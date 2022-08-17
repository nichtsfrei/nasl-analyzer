use tracing::warn;

use crate::openvas_funcs::OpenVASInBuildFunctions;

#[derive(Debug)]
pub struct Cache {
    pub paths: Vec<String>,
    internal: Option<OpenVASInBuildFunctions>,
}

impl Cache {
    pub fn update_paths(&mut self, paths: Vec<String>) {
        self.paths.extend(paths);
    }

    pub fn new(paths: Vec<String>) -> Cache {
        Cache {
            paths,
            internal: None,
        }
    }

    pub fn set_internal(&mut self, path: &str) {
        let vp = if path.ends_with(".c") {
            path.to_string()
        } else {
            format!("{}/nasl/nasl_init.c", path)
        };
        match OpenVASInBuildFunctions::from_path(&vp) {
            Ok(i) => self.internal = Some(i),
            Err(err) => warn!("enable to parse {path}: {err}"),
        }
    }

    pub fn internal(&mut self) -> Option<OpenVASInBuildFunctions> {
        self.internal.clone()
    }
}
