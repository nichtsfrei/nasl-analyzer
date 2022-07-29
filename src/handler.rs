use std::{error::Error, str::FromStr};

use lsp_server::{Connection, Message, RequestId, Response};
use nasl::{cache::Cache, interpret::FindDefinitionExt, types::to_pos};

use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Url};
use tracing::debug;
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
        let inter = self.update(path)?;
        let name = inter.identifier(path, line, character)?;
        debug!("looking for {}({line}:{character}) in {path}", name.name);

        let mut found: Vec<Location> = inter
            .find_definition(&name)
            .iter()
            .map(|p| Location {
                range: p.as_range(),
                uri: tdp.text_document.uri.clone(),
            })
            .collect();
        let fin: Vec<Location> = inter
            .includes()
            .flat_map(|i| self.find(i))
            .map(|(p, i)| (p, i.find_definition(&name)))
            .filter(|(_, v)| !v.is_empty())
            .flat_map(|(k, v)| {
                let result: Vec<Location> = v.iter().filter_map(|i| location(k, i)).collect();
                result
            })
            .collect();
        found.extend(fin);
        if found.is_empty() {
            let istr: Vec<String> = inter
                .calls("include")
                .filter(|(i, _)| i.in_pos(to_pos(line, character)))
                .flat_map(|(_, p)| {
                    let r: Vec<String> = p.iter().filter_map(|p| p.to_string()).collect();
                    r
                })
                .collect();
            let incs: Vec<Location> = istr
                .iter()
                .flat_map(|p| self.find(p))
                .filter_map(|(p, _)| location(p, &Point::default()))
                .collect();
            found.extend(incs);
        }
        if found.is_empty() {
            if let Some(i) = self.internal() {
                found.extend(
                    i.find_definition(&name)
                        .iter()
                        .filter_map(|(path, point)| location(path, point))
                        .collect::<Vec<Location>>(),
                )
            }
        }
        debug!("found goto definitions: {:?}", found);
        Some(GotoDefinitionResponse::Array(found))
    }
}
