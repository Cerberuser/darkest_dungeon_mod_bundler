use crate::bundler::{
    game_data::{Binary, Loadable},
    loader::utils::{ends_with, collect_paths},
};
use std::path::{Path, PathBuf};

mod hero_info;
pub use hero_info::HeroInfo;

pub struct HeroBinary(PathBuf);
impl Binary for HeroBinary {
    fn into_path(self) -> PathBuf {
        self.0
    }
}

impl Loadable for HeroBinary {
    fn load_raw(path: &Path) -> std::io::Result<Self> {
        Ok(Self(path.into()))
    }
    fn prepare_list(root_path: &Path) -> std::io::Result<Vec<PathBuf>> {
        collect_paths(&root_path.join("heroes"), |path| Ok(!ends_with(path, ".info.darkest")))
    }
}
