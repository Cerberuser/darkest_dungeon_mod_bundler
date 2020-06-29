use serde::de::DeserializeOwned;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub fn collect_paths(
    dir: &Path,
    predicate: impl (Fn(&Path) -> std::io::Result<bool>) + Clone,
) -> std::io::Result<Vec<PathBuf>> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.metadata()?.is_dir() {
            out.extend(collect_paths(&path, predicate.clone())?);
        } else if predicate(&path)? {
            out.push(path);
        }
    }
    Ok(out)
}

pub fn has_ext(path: impl AsRef<Path>, ext: &str) -> bool {
    path.as_ref().extension().and_then(OsStr::to_str) == Some(ext)
}

pub fn ends_with(path: impl AsRef<Path>, suffix: &str) -> bool {
    path.as_ref()
        .to_str()
        .map(|s| s.ends_with(suffix))
        .unwrap_or(false)
}

// This will be used, when we add more structural data entries.
#[allow(dead_code)]
pub fn load_json<T: DeserializeOwned>(path: &Path) -> std::io::Result<T> {
    match serde_json::from_reader(std::fs::File::open(&path)?) {
        Ok(json) => Ok(json),
        Err(error) => {
            panic!("Malformed JSON in file {:?}: {:?}", path, error);
        }
    }
}

pub fn rel_path(base_path: impl AsRef<Path>, path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let path = path.as_ref();
    path.strip_prefix(base_path)
        .map(PathBuf::from)
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Bundler reached the path outside of the working directory: {}",
                    path.to_string_lossy()
                ),
            )
        })
}
