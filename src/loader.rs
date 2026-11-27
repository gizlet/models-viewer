//! Reads and decodes models on a worker thread, so the interface keeps
//! moving while a GLB is parsed and its images are decoded. Only the newest
//! request is served: flicking through models skips the ones passed by.

use crate::scene::Scene;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

pub struct Loaded {
    pub index: usize,
    pub scene: Result<Scene, String>,
    pub seconds: f32,
}

pub struct Loader {
    requests: Sender<(usize, PathBuf)>,
    results: Receiver<Loaded>,
}

pub fn load(path: &PathBuf) -> Result<Scene, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Scene::from_glb(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

impl Loader {
    pub fn new() -> Loader {
        let (requests, inbox) = channel::<(usize, PathBuf)>();
        let (outbox, results) = channel();
        std::thread::Builder::new()
            .name("loader".into())
            .spawn(move || {
                while let Ok(mut job) = inbox.recv() {
                    while let Ok(newer) = inbox.try_recv() {
                        job = newer;
                    }
                    let start = Instant::now();
                    let scene = load(&job.1);
                    let loaded = Loaded { index: job.0, scene, seconds: start.elapsed().as_secs_f32() };
                    if outbox.send(loaded).is_err() {
                        break;
                    }
                }
            })
            .expect("loader thread");
        Loader { requests, results }
    }

    pub fn request(&self, index: usize, path: PathBuf) {
        let _ = self.requests.send((index, path));
    }

    pub fn poll(&self) -> Option<Loaded> {
        let mut newest = None;
        while let Ok(l) = self.results.try_recv() {
            newest = Some(l);
        }
        newest
    }
}
