use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use tracing::{debug, warn};
use walkdir::WalkDir;

use crate::{
    interpret::{self, Interpret},
    openvas_funcs,
};

#[derive(Debug)]
pub struct Cache {
    paths: Vec<String>,
    plugins: HashMap<String, Interpret>,
    internal: Option<openvas_funcs::Interpreter>,
}

impl Cache {
    fn load_plugins(paths: Vec<String>) -> HashMap<String, Interpret> {
        let worker_count = num_cpus::get();
        let (tx, rx) = mpsc::channel();
        let mut children = Vec::new();
        for _ in 0..worker_count {
            let (txp, rxp): (Sender<String>, Receiver<String>) = mpsc::channel();
            let ttx = tx.clone();
            let child = thread::spawn(move || {
                while let Ok(x) = rxp.recv_timeout(Duration::from_secs(1)) {
                    match interpret::from_path(&x) {
                        Ok(inter) => {
                            if let Err(err) = ttx.send((x.clone(), inter)) {
                                warn!("unable to send {x} for caching: {err}");
                            }
                        }
                        Err(err) => warn!("unable to create inter {x}: {err}"),
                    }
                }
            });
            children.push((txp, child));
        }

        let mut plugins = HashMap::new();
        for path in paths {
            let np = path.strip_prefix("file://").unwrap_or(&path);
            debug!("looking for .nasl or .inc in {np}");
            for (i, r) in WalkDir::new(np).follow_links(true).into_iter().enumerate() {
                match r {
                    Ok(entry) => {
                        let fname = entry.path().to_string_lossy();
                        if fname.ends_with(".inc") || fname.ends_with(".nasl") {
                            if let Err(err) = children[i % worker_count].0.send(fname.to_string()) {
                                warn!(
                                    "unable to send {fname} to child {}: {err}",
                                    i % worker_count
                                );
                            }
                        }
                    }
                    Err(err) => warn!("unable to load file {err}"),
                }
            }
        }
        let stopped = || -> bool {
            children
                .iter()
                .map(|(_, c)| c.is_finished())
                .reduce(|p, n| p && n)
                .unwrap_or(false)
        };
        while !stopped() {
            while let Ok((p, i)) = rx.recv_timeout(Duration::from_millis(10)) {
                plugins.insert(p, i);
            }
        }
        plugins
    }

    pub fn update_paths(&mut self, paths: Vec<String>) {
        let plugins = Cache::load_plugins(paths.clone());
        self.paths.extend(paths);
        self.plugins.extend(plugins);
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    pub fn new(paths: Vec<String>) -> Cache {
        let plugins = Cache::load_plugins(paths.clone());
        Cache {
            plugins,
            paths,
            internal: None,
        }
    }

    pub fn update(&mut self, path: &str) -> Option<Interpret> {
        match interpret::from_path(path) {
            Ok(inter) => {
                self.plugins.insert(path.to_string(), inter.clone());
                Some(inter)
            }
            Err(err) => {
                warn!("unable to uypdate {path}: {err}");
                None
            }
        }
    }

    pub fn set_internal(&mut self, path: &str) {
        let vp = if path.ends_with(".c") {
            path.to_string()
        } else {
            format!("{}/nasl/nasl_init.c", path)
        };
        match openvas_funcs::Interpreter::from_path(&vp) {
            Ok(i) => self.internal = Some(i),
            Err(err) => warn!("enable to parse {path}: {err}"),
        }
    }

    pub fn internal(&mut self) -> Option<openvas_funcs::Interpreter> {
        self.internal.clone()
    }

    pub fn get(self, path: &str) -> Option<Interpret> {
        self.plugins.get(path).cloned()
    }

    pub fn find<'a>(&'a self, path: &'a str) -> impl Iterator<Item = (&String, &Interpret)> + 'a {
        self.plugins.iter().filter_map(move |(k, v)| {
            if k.ends_with(path) {
                return Some((k, v));
            }
            None
        })
    }

    pub fn each<'a>(&'a self) -> impl Iterator<Item = (&String, &Interpret)> + 'a {
        self.plugins.iter()
    }
}
