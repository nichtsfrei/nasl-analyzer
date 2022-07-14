use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use tracing::{debug, warn};
use walkdir::WalkDir;

use crate::interpret::{self, Interpret};

#[derive(Debug)]
pub struct Cache {
    paths: Vec<String>,
    plugins: HashMap<String, Interpret>,
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
                    let inter = interpret::from_path(&x).expect("Expected parsed interpreter");
                    if let Err(err) = ttx.send((x.clone(), inter)) {
                        eprintln!("Unable to send {x} for caching. {err}");
                    }
                }
            });
            children.push((txp, child));
        }

        let mut plugins = HashMap::new();
        for path in paths {
            // TODO remove file:// prefix here
            let np = path.strip_prefix("file://").unwrap_or(&path);
            debug!("looking for .nasl or .inc in {np}");
            for (i, r) in WalkDir::new(np).follow_links(true).into_iter().enumerate() {
                match r {
                    Ok(entry) => {
                        let fname = entry.path().to_string_lossy();
                        // we only care for inc files due to nasl limitation on include
                        if fname.ends_with(".inc") || fname.ends_with(".nasl") {
                            if let Err(err) = children[i % worker_count].0.send(fname.to_string()) {
                                eprintln!(
                                    "Unable to send {fname} to child {}. {err}",
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
        Cache { plugins, paths }
    }

    pub fn update(&mut self, path: &str) -> Option<Interpret> {
        let inter = interpret::from_path(path).expect("Expected parsed interpreter");
        self.plugins.insert(path.to_string(), inter.clone());
        Some(inter)
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
}
