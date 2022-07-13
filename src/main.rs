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
mod extension;
mod handler;
use std::error::Error;

use lsp_types::OneOf;
use lsp_types::{request::GotoDefinition, InitializeParams, ServerCapabilities};

use nasl::cache::Cache;

use lsp_server::{Connection, ExtractError, Message, Request, RequestId};
use tracing::{debug, info, Level};

use crate::extension::Settings;
use crate::handler::RequestResponseSender;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_max_level(Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    

    info!("Starting nasl-analyzer");
    let (connection, io_threads) = Connection::stdio();
    let (id, params) = connection.initialize_start()?;
    let init_params: InitializeParams = serde_json::from_value(params).unwrap();
    let server_capabilities = ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };

    let initialize_data = serde_json::json!({
        "capabilities": server_capabilities,
        "serverInfo": {
          "name": "nasl-analyzer",
          "version": "0.1",
    }});
    connection.initialize_finish(id, initialize_data)?;

    main_loop(connection, init_params)?;
    io_threads.join()?;

    // Shut down gracefully.
    eprintln!("shutting down server");
    Ok(())
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
    let rrs = RequestResponseSender {
        connection: &connection,
    };
    debug!("Initialized cache ({}) for {:?}", cache.count(), rp);
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }

                match cast::<GotoDefinition>(req) {
                    Ok((id, params)) => {
                        rrs.send_response(&mut cache, params, id)?;
                        continue;
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{:?}", err),
                    Err(ExtractError::MethodMismatch(req)) => req,
                };
                // ...
            }
            Message::Response(resp) => {
                debug!("got response: {:?}", resp);
            }
            Message::Notification(not) => {
                if not.method == "workspace/didChangeConfiguration" {
                    let set: Result<Settings, serde_json::Error> =
                        serde_json::from_value(not.clone().params);
                    if let Ok(set) = set {
                        let paths = set.settings.clone().map(|i| i.paths).unwrap_or_default();
                        cache.update_paths(paths);
                        debug!("Updated cache ({}) for {:?}", cache.count(), set.settings);
                    }
                } else {
                    debug!("got notification: {:?}", not);
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
