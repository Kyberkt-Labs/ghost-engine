use std::path::PathBuf;
use std::sync::Mutex;
use std::{env, fs};

use servo::resources::{self, Resource};

static RESOURCE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

struct GhostResourceReader;

pub(crate) fn init() {
    resources::set(Box::new(GhostResourceReader));
}

fn resources_dir_path() -> PathBuf {
    let mut dir = RESOURCE_DIR.lock().unwrap();
    if let Some(ref path) = *dir {
        return path.clone();
    }

    // Walk up from the executable to find the resources/ directory.
    let mut path = env::current_exe().unwrap().canonicalize().unwrap();
    while path.pop() {
        path.push("resources");
        if path.is_dir() {
            *dir = Some(path.clone());
            return path;
        }
        path.pop();

        path.push("Resources");
        if path.is_dir() {
            *dir = Some(path.clone());
            return path;
        }
        path.pop();
    }

    // Fallback: walk up from cwd (dev builds only).
    let mut path = env::current_dir().unwrap();
    loop {
        path.push("resources");
        if path.is_dir() {
            *dir = Some(path.clone());
            return path;
        }
        path.pop();
        if !path.pop() {
            panic!("Cannot find Servo resources/ directory");
        }
    }
}

impl resources::ResourceReaderMethods for GhostResourceReader {
    fn read(&self, file: Resource) -> Vec<u8> {
        let mut path = resources_dir_path();
        path.push(file.filename());
        fs::read(&path).unwrap_or_else(|e| panic!("Cannot read resource {}: {e}", path.display()))
    }

    fn sandbox_access_files_dirs(&self) -> Vec<PathBuf> {
        vec![resources_dir_path()]
    }

    fn sandbox_access_files(&self) -> Vec<PathBuf> {
        vec![]
    }
}
