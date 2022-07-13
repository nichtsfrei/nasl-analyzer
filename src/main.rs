mod extension;
mod handler;
use std::env;
use std::error::Error;
use std::fs::File;

use lsp_types::OneOf;
use lsp_types::{request::GotoDefinition, InitializeParams, ServerCapabilities};

use nasl::cache::Cache;

use lsp_server::{Connection, ExtractError, Message, Request, RequestId};
use tracing::{debug, info, Level};

use crate::extension::Settings;
use crate::handler::RequestResponseSender;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let home = env::var("HOME")?;
    let file = File::create(format!("{home}/.cache/nvim/nasl-analyzer.log"))?;
    let subscriber = tracing_subscriber::fmt()
        .with_writer(file)
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
