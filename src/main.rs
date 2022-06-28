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
use std::collections::HashMap;
use std::error::Error;
use std::fs;

use lsp_types::{
    request::GotoDefinition, GotoDefinitionResponse, InitializeParams, ServerCapabilities,
};
use lsp_types::{GotoDefinitionParams, Location, OneOf, Position, Range, Url};

use tree_sitter::{Node, Parser, Point, Tree};

use lsp_server::{Connection, ExtractError, Message, Request, RequestId, Response};

fn node_to_identifier(code: String, i: Node) -> Option<(String, Point)> {
    let icrsr = &mut i.walk();
    let icidx = i.named_children(icrsr);
    let fid = icidx
        .filter(|i| i.kind() == "identifier")
        .map(|i| (&code[i.byte_range()], i.range().start_point))
        .map(|(k, v)| (k.to_string(), v));
    fid.last()
}

fn node_functions(code: String, node: Node) -> HashMap<String, Point> {
    let rcrsr = &mut node.walk();
    let crsr = node.named_children(rcrsr);
    let ffuncimpl = crsr
        .filter(|i| i.kind() == "func_impl")
        .map(|i| node_to_identifier(code.clone(), i).unwrap_or_default());
    ffuncimpl.collect()
}

fn parse_file(mut parser: Parser, url: Url) -> (String, Tree) {
    let code: String =
        fs::read_to_string(url.path()).expect("Something went wrong reading the file");
    let parsed = parser.parse(code.clone(), None).unwrap();
    (code, parsed)
}

fn identifier_in_posiion(
    code: String,
    node: Node,
    params: GotoDefinitionParams,
) -> Option<(String, Point)> {
    let rcrsr = &mut node.walk();
    let pos = params.text_document_position_params.position.line as f32
        + params.text_document_position_params.position.character as f32 / 100.0;
    for i in node.children(rcrsr) {
        let sp = i.range().start_point.row as f32 + i.range().start_point.column as f32 / 100.0;
        let ep = i.range().end_point.row as f32 + i.range().end_point.column as f32 / 100.0;
        if pos >= sp && pos <= ep {
            // in here check if cursor sits on an identifier
            return node_to_identifier(code, i);
        }
    }
    None
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    // Note that  we must have our logging only write out to stderr.
    eprintln!("starting generic LSP server");
    let code = r#"
        function test(a){
            return 3;
        }
    "#;
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_nasl::language())
        .expect("Error loading NASL grammar");

    let parsed = parser.parse(code, None).unwrap();
    let fhm = node_functions(code.to_string(), parsed.root_node());
    eprintln!("{:?}", fhm);

    //println!("{:#?}", parsed.root_node().to_sexp());

    // Create the transport. Includes the stdio (stdin and stdout) versions but this could
    // also be implemented to use sockets or HTTP.
    let (connection, io_threads) = Connection::stdio();

    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })
    .unwrap();
    let initialization_params = connection.initialize(server_capabilities)?;
    main_loop(connection, initialization_params)?;
    io_threads.join()?;

    // Shut down gracefully.
    eprintln!("shutting down server");
    Ok(())
}

fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();
    eprintln!("starting example main loop");
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
                        let mut parser = Parser::new();
                        parser
                            .set_language(tree_sitter_nasl::language())
                            .expect("Error loading NASL grammar");
                        // add cache lookup for previous tree
                        let (code, tree) = parse_file(
                            parser,
                            params
                                .clone()
                                .text_document_position_params
                                .text_document
                                .uri,
                        );
                        let fhm = node_functions(code.clone(), tree.root_node());
                        let idpos = identifier_in_posiion(code, tree.root_node(), params.clone());
                        let found = {
                            match idpos.map(|(id, _)| fhm.get(&id)) {
                                Some(x) => x,
                                None => None,
                            }
                        }
                        .map(|p| Location {
                            range: Range {
                                start: Position {
                                    line: p.row as u32,
                                    character: p.column as u32,
                                },
                                end: Position {
                                    line: p.row as u32,
                                    character: p.column as u32,
                                },
                            },
                            uri: params.text_document_position_params.text_document.uri,
                        });
                        let result: Vec<Location> = match found {
                            Some(x) => vec![x],
                            None => vec![],
                        };

                        let result = Some(GotoDefinitionResponse::Array(result));
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
                eprintln!("got notification: {:?}", not);
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
