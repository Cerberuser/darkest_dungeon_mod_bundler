use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub fn dir_to_binary(
    base_path: impl AsRef<Path>,
    dir: impl AsRef<Path>,
    predicate: impl Fn(&Path) -> bool,
) -> std::io::Result<HashMap<PathBuf, PathBuf>> {
    let base_path = base_path.as_ref();
    filter_dir(dir, predicate).and_then(|v| {
        v.into_iter()
            .map(|path| Ok((rel_path(base_path, &path)?, path)))
            .collect()
    })
}

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

pub fn load_by_path<R>(base_path: &Path, path: &Path, loader: impl FnOnce(&Path) -> std::io::Result<R>) -> std::io::Result<(PathBuf, R)> {
    loader(&path).and_then(|data| {
        rel_path(&base_path, path).map(|path| (path, data))
    })
}

pub fn collect_tree<R>(
    base_path: &Path,
    dir: &Path,
    mut fun: impl FnMut(&Path) -> std::io::Result<Option<R>> + Clone,
) -> std::io::Result<HashMap<PathBuf, R>> {
    let mut collection = HashMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.metadata()?.is_dir() {
            collection.extend(collect_tree(&base_path, &path, fun.clone())?);
        } else {
            collection.extend(fun(&path).and_then(|data| {
                rel_path(&base_path, path).map(|path| data.map(|data| (path, data)))
            })?);
        }
    }
    Ok(collection)
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

pub fn filter_dir(
    dir: impl AsRef<Path>,
    predicate: impl Fn(&Path) -> bool,
) -> std::io::Result<Vec<PathBuf>> {
    std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .filter(|path| path.as_ref().map(|path| predicate(path)).unwrap_or(true))
        .collect()
}

pub fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> std::io::Result<T> {
    match serde_json::from_reader(std::fs::File::open(&path)?) {
        Ok(json) => Ok(json),
        Err(error) => {
            panic!("Malformed JSON in file {:?}: {:?}", path.as_ref(), error);
        }
    }
}

pub fn read_from_json<T: DeserializeOwned>(
    base_path: impl AsRef<Path>,
    paths: impl IntoIterator<Item = PathBuf>,
) -> std::io::Result<HashMap<PathBuf, T>> {
    let base_path = base_path.as_ref();
    paths
        .into_iter()
        .map(|path| {
            Ok((
                rel_path(base_path, &path)?,
                match serde_json::from_reader(std::fs::File::open(path)?) {
                    Ok(json) => json,
                    Err(error) => {
                        panic!("Malformed JSON: {:?}", error);
                    }
                },
            ))
        })
        .collect()
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
