use std::{error::Error, str::FromStr};

use lsp_server::{Connection, Message, RequestId, Response};
use nasl::cache::Cache;

use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Url};

use crate::extension::AsRange;

pub trait ToResponse<T, R> {
    fn handle(&mut self, params: T) -> Option<R>;
}
pub struct RequestResponseSender<'a> {
    pub connection: &'a Connection,
}

impl<'a> RequestResponseSender<'a> {
    pub fn send_response<T, R>(
        &self,
        to_response: &mut dyn ToResponse<T, R>,
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

impl ToResponse<GotoDefinitionParams, GotoDefinitionResponse> for Cache {
    fn handle(&mut self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let tdp = params.text_document_position_params;

        let line = tdp.position.line as usize;
        let character = tdp.position.character as usize;
        let inter = self.update(tdp.text_document.uri.path())?;
        let name = inter.identifier(line, character)?;

        let mut found: Vec<Location> = inter
            .find_definition(&name, line, character)
            .iter()
            .map(|p| Location {
                range: p.as_range(),
                uri: tdp.text_document.uri.clone(),
            })
            .collect();
        let fin: Vec<Location> = inter
            .includes()
            .flat_map(|i| self.find(i))
            .map(|(p, i)| (p, i.find_definition(&name, line, character)))
            .filter(|(_, v)| !v.is_empty())
            .flat_map(|(k, v)| {
                let result: Vec<Location> = v
                    .iter()
                    .filter_map(|i| {
                        if let Ok(val) = Url::from_str(&format!("file://{}", k.clone())) {
                            return Some(Location {
                                range: i.as_range(),
                                uri: val,
                            });
                        }
                        None
                    })
                    .collect();
                result
            })
            .collect();
        found.extend(fin);
        Some(GotoDefinitionResponse::Array(found))
    }
}
