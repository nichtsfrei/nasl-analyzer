//! A minimal example LSP server that can only respond to the `gotoDefinition` request. To use
//! this example, execute it and then send an `initialize` request.
//!
//! ```no_run
//! Content-Length: 85
//!
//! {"jsonrpc": "2.0", "method": "initialize", "id": 1, "params": {"capabilities": {}}}
//! ```
//!
//! This will respond with a server response. Then send it a `initialized` notification which will
//! have no response.
//!
//! ```no_run
//! Content-Length: 59
//!
//! {"jsonrpc": "2.0", "method": "initialized", "params": {}}
//! ```
//!
//! Once these two are sent, then we enter the main loop of the server. The only request this
//! example can handle is `gotoDefinition`:
//!
//! ```no_run
//! Content-Length: 159
//!
//! {"jsonrpc": "2.0", "method": "textDocument/definition", "id": 2, "params": {"textDocument": {"uri": "file://temp"}, "position": {"line": 1, "character": 1}}}
//! ```
//!
//! To finish up without errors, send a shutdown request:
//!
//! ```no_run
//! Content-Length: 67
//!
//! {"jsonrpc": "2.0", "method": "shutdown", "id": 3, "params": null}
//! ```
//!
//! The server will exit the main loop and finally we send a `shutdown` notification to stop
//! the server.
//!
//! ```
//! Content-Length: 54
//!
//! {"jsonrpc": "2.0", "method": "exit", "params": null}
//! ```
use std::error::Error;
use std::str::FromStr;

use lsp_types::{
    request::GotoDefinition, GotoDefinitionResponse, InitializeParams, ServerCapabilities,
};
use lsp_types::{Location, OneOf, Position, Range, Url};

use nasl::cache::Cache;
use serde::{Deserialize, Serialize};
use tree_sitter::Point;

use lsp_server::{Connection, ExtractError, Message, Request, RequestId, Response};

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct Paths {
    paths: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Paths>,
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    // Note that  we must have our logging only write out to stderr.

    eprintln!("starting generic LSP server");
    // Create the transport. Includes the stdio (stdin and stdout) versions but this could
    // also be implemented to use sockets or HTTP.
    let (connection, io_threads) = Connection::stdio();

    // Run the server
    let (id, params) = connection.initialize_start()?;

    let init_params: InitializeParams = serde_json::from_value(params).unwrap();
    //let client_capabilities: ClientCapabilities = init_params.capabilities;
    let server_capabilities = ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };

    let initialize_data = serde_json::json!({
        "capabilities": server_capabilities,
        "serverInfo": {
        "name": "nasl-analyzer",
        "version": "0.1"
    }
    });
    connection.initialize_finish(id, initialize_data)?;

    // parse and then initialize cache
    main_loop(connection, init_params)?;
    io_threads.join()?;

    // Shut down gracefully.
    eprintln!("shutting down server");
    Ok(())
}

trait AsRange {
    fn as_range(&self) -> Range;
}

impl AsRange for Point {
    fn as_range(&self) -> Range {
        Range {
            start: Position {
                line: self.row as u32,
                character: self.column as u32,
            },
            end: Position {
                line: self.row as u32,
                character: self.column as u32,
            },
        }
    }
}

fn main_loop(
    connection: Connection,
    init_params: InitializeParams,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let rp: Vec<String> = init_params
        .workspace_folders
        .map(|i| i.iter().map(|i| i.uri.to_string()).collect())
        .unwrap_or_default();
    let mut cache = Cache::new(rp.clone());
    eprintln!("cache size: {} for {:?}", cache.count(), rp);
    //let p: InitializeParams = serde_json::from_value(params).unwrap();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }

                match cast::<GotoDefinition>(req) {
                    Ok((id, params)) => {
                        //params.text_document_position_params.text_document.uri
                        // load file
                        let tdp = params.text_document_position_params;

                        let inter = cache
                            .update(tdp.text_document.uri.path())
                            .expect("Expected parsed interpreter");
                        let name = inter.identifier(
                                tdp.position.line as usize,
                                tdp.position.character as usize,
                            ).unwrap_or_default();
                        // TODO speed up; going through all incs takes too long
                        let mut found: Vec<Location> = inter
                            .find_definition(
                                &name,
                                tdp.position.line as usize,
                                tdp.position.character as usize,
                            )
                            .iter()
                            .map(|p| Location {
                                range: p.as_range(),
                                uri: tdp.text_document.uri.clone(),
                            })
                            .collect();
                        let fin: Vec<Location> = inter
                            .includes()
                            // maybe just use first?
                            .flat_map(|i| cache.find(i))
                            .map(|(p, i)| {
                                (
                                    p,
                                    i.find_definition(
                                        &name,
                                        tdp.position.line as usize,
                                        tdp.position.character as usize,
                                    ),
                                )
                            })
                            .filter(|(_, v)| !v.is_empty())
                            .flat_map(|(k, v)| {
                                let result : Vec<Location> = v.iter().map(|i| Location {
                                    range: i.as_range(),
                                    uri: Url::from_str(&format!("file://{}", k.clone()))
                                        .expect("unable to parse file://{k}"),
                                }).collect();
                                result
                            })
                            .collect();
                        found.extend(fin);
                        let result = Some(GotoDefinitionResponse::Array(found));
                        let result = serde_json::to_value(&result).unwrap();
                        let resp = Response {
                            id,
                            result: Some(result),
                            error: None,
                        };
                        connection.sender.send(Message::Response(resp))?;
                        continue;
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{:?}", err),
                    Err(ExtractError::MethodMismatch(req)) => req,
                };
                // ...
            }
            Message::Response(resp) => {
                eprintln!("got response: {:?}", resp);
            }
            Message::Notification(not) => {
                if not.method == "workspace/didChangeConfiguration" {
                    let set: Result<Settings, serde_json::Error> =
                        serde_json::from_value(not.clone().params);
                    if let Ok(set) = set {
                        let paths = set.settings.map(|i| i.paths).unwrap_or_default();
                        cache.update_paths(paths);
                        eprintln!("cache size: {}", cache.count());
                    }
                } else {
                    eprintln!("got notification: {:?}", not);
                }
            }
        }
    }
    Ok(())
}

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}
