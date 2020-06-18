use std::path::{Path, PathBuf};

pub fn workshop(base: impl AsRef<Path>) -> PathBuf {
    base.as_ref().join("steamapps/workshop/content/262060")
}

pub fn game(base: impl AsRef<Path>) -> PathBuf {
    base.as_ref().join("steamapps/common/DarkestDungeon")
}

pub fn mods(base: impl AsRef<Path>) -> PathBuf {
    game(base).join("mods")
}
