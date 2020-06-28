use super::super::{Binary, Loadable};
use crate::bundler::loader::utils::{collect_paths, collect_tree};
use std::{
    collections::HashMap,
    io::Result as IoResult,
    path::{Path, PathBuf},
};

pub struct ActivityLogImage(PathBuf);
impl Binary for ActivityLogImage {
    fn into_path(self) -> PathBuf {
        self.0
    }
}

impl Loadable for ActivityLogImage {
    fn load_raw(
        path: &Path,
    ) -> std::io::Result<Self> {
        Ok(Self(path.into()))
    }
    fn prepare_list(root_path: &Path) -> std::io::Result<Vec<PathBuf>> {
        collect_paths(
            &root_path.join("activity_log"),
            |_| Ok(true),
        )
    }
}