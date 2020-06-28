use super::super::{
    btree_vec, BTreeLinkedMappable, BTreeMapExt, BTreeMappable, BTreePatchable, Binary, Loadable,
    RestMap,
};
use crate::bundler::{
    diff::DataMap,
    loader::utils::{collect_tree, ends_with, has_ext, load_json, collect_paths},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Result as IoResult,
    path::{Path, PathBuf},
};

pub struct AudioData(PathBuf);
impl Binary for AudioData {
    fn into_path(self) -> PathBuf {
        self.0
    }
}

impl Loadable for AudioData {
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
