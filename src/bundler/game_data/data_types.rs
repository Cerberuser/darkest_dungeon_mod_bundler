use super::{Binary, Loadable};
use crate::bundler::loader::utils::collect_paths;
use std::path::{Path, PathBuf};

mod heroes;
mod localization;

pub use heroes::{HeroBinary, HeroInfo};
pub use localization::StringsTable;

pub struct BinaryData(PathBuf);

lazy_static::lazy_static! {
    static ref BINARY_DIRS: Vec<&'static str> = vec![
        "activity_log",
        "dungeons",
        "fx",
        "loading_screen",
        "modes",
        "panels",
        "raid_results",
        "audio",
        "curios",
        "effects",
        "game_over",
        "scripts",
        "video",
        "campaign",
        "cursors",
        "fe_flow",
        "loot",
        "monsters",
        "props",
        "scrolls",
        "shared",
        "trinkets",
        "colours",
        "fonts",
        "inventory",
        "maps",
        "overlays",
        "raid",
        "upgrades",
    ];
}

impl Binary for BinaryData {
    fn into_path(self) -> PathBuf {
        self.0
    }
}

impl Loadable for BinaryData {
    fn load_raw(path: &Path) -> std::io::Result<Self> {
        Ok(Self(path.into()))
    }
    fn prepare_list(root_path: &Path) -> std::io::Result<Vec<PathBuf>> {
        let results: Result<Vec<Vec<_>>, _> = BINARY_DIRS
            .iter()
            .map(|dir| root_path.join(dir))
            .inspect(|dir| log::debug!("Loading binary data from {:?}", dir))
            .map(|dir| collect_paths(&dir, |_| Ok(true)))
            .collect();
        Ok(results?.into_iter().flatten().collect())
    }
}
