use std::{error::Error, path::Path, str::FromStr};

use lsp_server::{Connection, Message, RequestId, Response};
use nasl::{
    cache::Cache,
    interpret::{self, FindDefinitionExt, NASLInterpreter},
};

use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Url};
use tracing::{debug, warn, trace};
use tree_sitter::Point;

use crate::extension::AsRangeExt;

pub trait ToResponseExt<T, R> {
    fn handle(&mut self, params: T) -> Option<R>;
}
pub struct RequestResponseSender<'a> {
    pub connection: &'a Connection,
}

impl<'a> RequestResponseSender<'a> {
    pub fn send_response<T, R>(
        &self,
        to_response: &mut dyn ToResponseExt<T, R>,
        params: T,
        id: RequestId,
    ) -> Result<(), Box<dyn Error + Sync + Send>>
    where
        R: serde::Serialize,
    {
        let result = to_response.handle(params);
        let resp = Response {
            id,
            result: serde_json::to_value(&result).ok(),
            error: None,
        };
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }
}

fn location(path: String, point: &Point) -> Option<Location> {
    if let Ok(val) = Url::from_str(&format!("file://{}", path)) {
        return Some(Location {
            range: point.as_range(),
            uri: val,
        });
    }
    None
}

fn interprete(
    path: &str,
    paths: Vec<String>,
    code: Option<&str>,
) -> Result<Vec<NASLInterpreter>, Box<dyn Error>> {
    let init = if let Some(code) = code {
        interpret::new(path, code)
    } else {
        let code = NASLInterpreter::read(path)?;
        interpret::new(path, &code)
    }?;
    let pths = paths.clone();
    let incs: Vec<NASLInterpreter> = init
        .includes()
        .flat_map(|i| pths.iter().map(|p| format!("{p}/{}", i.clone())))
        .map(|p| p.strip_prefix("file://").unwrap_or(&p).to_string())
        .filter(|p| {
            Path::new(p).exists()
        })
        .flat_map(|p| {
            trace!("parsing {p}");
            interprete(&p, paths.clone(), None)
        })
        .flatten()
        .collect();
    let mut result = vec![init];
    result.extend(incs);
    Ok(result)
}

impl ToResponseExt<GotoDefinitionParams, GotoDefinitionResponse> for Cache {
    fn handle(&mut self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let tdp = params.text_document_position_params;

        let line = tdp.position.line as usize;
        let character = tdp.position.character as usize;
        let path = tdp.text_document.uri.path();
        let code = match NASLInterpreter::read(path) {
            Ok(c) => Some(c),
            Err(err) => {
                warn!("unable to load {path}: {err}");
                None
            }
        }?;
        let sp = NASLInterpreter::identifier(path, &code, line, character)?;
        let interprets: Vec<NASLInterpreter> = match interprete(path, self.paths.clone(), Some(&code)) {
            Ok(i) => {
                debug!("found {} interpreter", i.len());
                i
            }
            Err(err) => {
                warn!("no interpreter found for {path}: {err}");
                vec![]
            }
        };
        debug!("looking for {}({line}:{character}) in {path}", sp.name);
        let mut found: Vec<Location> = interprets
            .iter()
            .map(|i| (i.clone().origin(), i.find_definition(&sp)))
            .flat_map(|(origin, locations)| {
                locations
                    .iter()
                    .filter_map(|i| location(origin.clone(), i))
                    .collect::<Vec<Location>>()
            })
            .collect();

        if found.is_empty() {
            if let Some(i) = self.internal() {
                found.extend(
                    i.find_definition(&sp)
                        .iter()
                        .filter_map(|(path, point)| location(path.to_string(), point))
                        .collect::<Vec<Location>>(),
                )
            }
        }
        debug!("found goto definitions: {:?}", found);
        Some(GotoDefinitionResponse::Array(found))
    }
}
