use std::{error::Error, str::FromStr};

use lsp_server::{Connection, Message, RequestId, Response};
use nasl::{
    cache::Cache,
    interpret::NASLDefinitions,
};

use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Url};
use tracing::{debug, warn};
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

fn location(path: &str, point: &Point) -> Option<Location> {
    if let Ok(val) = Url::from_str(&format!("file://{}", path)) {
        return Some(Location {
            range: point.as_range(),
            uri: val,
        });
    }
    None
}

impl ToResponseExt<GotoDefinitionParams, GotoDefinitionResponse> for Cache {
    fn handle(&mut self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let tdp = params.text_document_position_params;

        let line = tdp.position.line as usize;
        let character = tdp.position.character as usize;
        let path = tdp.text_document.uri.path();
        let code = match NASLDefinitions::read(path) {
            Ok(c) => Some(c),
            Err(err) => {
                warn!("unable to load {path}: {err}");
                None
            }
        }?;
        let sp = NASLDefinitions::search_parameter(path, &code, line, character)?;
        let interprets: Vec<NASLDefinitions> =
            match NASLDefinitions::new_with_includes(path, self.paths.clone(), Some(&code)) {
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
            .flat_map(|i| {
                let origin = i.clone().origin();
                i.find_points(&sp)
                    .filter_map(|i| location(&origin, &i))
                    .collect::<Vec<Location>>()
            })
            .collect();

        if found.is_empty() {
            if let Some(i) = self.internal() {
                found.extend(
                    i.find_origin_location(&sp)
                        .filter_map(|(path, point)| location(&path, &point))
                        .collect::<Vec<Location>>(),
                )
            }
        }
        debug!("found goto definitions: {:?}", found);
        Some(GotoDefinitionResponse::Array(found))
    }
}
