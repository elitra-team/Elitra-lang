use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind};

pub struct PackageManager {
    search_dirs: Vec<PathBuf>,
    std_modules: HashMap<String, String>,
}

impl PackageManager {
    pub fn new() -> Self {
        let mut search_dirs = Vec::new();
        if let Ok(dir) = std::env::current_dir() {
            search_dirs.push(dir);
        }
        if let Some(home) = dirs_data() {
            search_dirs.push(home.join("packages"));
        }
        PackageManager {
            search_dirs,
            std_modules: HashMap::new(),
        }
    }

    pub fn register_std_module(&mut self, name: &str, source: &str) {
        self.std_modules
            .insert(name.to_string(), source.to_string());
    }

    pub fn resolve(&self, path: &str, current_file: Option<&str>) -> Result<String, Error> {
        if path.starts_with("std/") {
            let name = path.strip_prefix("std/").unwrap();
            if self.std_modules.contains_key(name) {
                return Ok(path.to_string());
            }
            return Err(Error::new(
                ErrorKind::Import,
                format!("Unknown std module '{}'", name),
            ));
        }
        if path.starts_with("./") || path.starts_with("../") || path.contains('/') {
            let cwd = current_file
                .and_then(|f| Path::new(f).parent())
                .unwrap_or_else(|| Path::new("."));
            let full_path = cwd.join(path);
            if full_path.exists() {
                return Ok(full_path.to_string_lossy().to_string());
            }
            let eltr = full_path.with_extension("eltr");
            if eltr.exists() {
                return Ok(eltr.to_string_lossy().to_string());
            }
            return Err(Error::new(
                ErrorKind::Import,
                format!("Module not found: '{}'", path),
            ));
        }
        for dir in &self.search_dirs {
            let candidates = [
                dir.join(path).with_extension("eltr"),
                dir.join(path).join(path).with_extension("eltr"),
                dir.join(format!("{}.eltr", path)),
            ];
            for c in &candidates {
                if c.exists() {
                    return Ok(c.to_string_lossy().to_string());
                }
            }
        }
        Err(Error::new(
            ErrorKind::Import,
            format!("Module not found: '{}'", path),
        ))
    }

    pub fn load_source(&self, resolved: &str) -> Result<String, Error> {
        if resolved.starts_with("std/") {
            let name = resolved.strip_prefix("std/").unwrap();
            if let Some(source) = self.std_modules.get(name) {
                return Ok(source.clone());
            }
        }
        fs::read_to_string(Path::new(resolved)).map_err(|e| {
            Error::new(
                ErrorKind::IO,
                format!("Could not read '{}': {}", resolved, e),
            )
        })
    }
}

fn dirs_data() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        Some(Path::new(&home).join(".eltr"))
    } else if let Ok(home) = std::env::var("USERPROFILE") {
        Some(Path::new(&home).join(".eltr"))
    } else {
        None
    }
}


