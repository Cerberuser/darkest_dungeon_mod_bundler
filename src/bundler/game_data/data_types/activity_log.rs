use super::super::{Binary, Loadable};
use crate::bundler::loader::utils::collect_tree;
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
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<HashMap<PathBuf, Self>> {
        collect_tree(
            root_path.as_ref(),
            &root_path.as_ref().join("activity_log"),
            move |path| {
                on_load(path.to_string_lossy().into());
                Ok(Some(ActivityLogImage(path.to_path_buf())))
            },
        )
    }
}
