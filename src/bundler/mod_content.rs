use std::{path::PathBuf, collections::HashMap};
use super::diff::Patch;
use super::game_data::{GameData, StructuredItem, BTreeMappable};

pub type ModBinaries = HashMap<PathBuf, PathBuf>;
pub type ModAddedTexts = HashMap<PathBuf, StructuredItem>;
pub type ModModifiedTexts = HashMap<PathBuf, Patch>;

#[derive(Clone, Debug)]
pub struct ModContent {
    binary: ModBinaries,
    text_added: ModAddedTexts,
    text_modified: ModModifiedTexts,
}

impl ModContent {
    pub fn build(binary: ModBinaries, text_added: ModAddedTexts, text_modified: ModModifiedTexts) -> Self {
        Self { binary, text_added, text_modified }
    }
    pub fn binary_ref(&self) -> &ModBinaries {
        &self.binary
    }
    pub fn text_added_ref(&self) -> &ModAddedTexts {
        &self.text_added
    }
    pub fn text_modified_ref(&self) -> &ModModifiedTexts {
        &self.text_modified
    }
    pub fn binary_mut(&mut self) -> &mut ModBinaries {
        &mut self.binary
    }
    pub fn text_added_mut(&mut self) -> &mut ModAddedTexts {
        &mut self.text_added
    }
    pub fn text_modified_mut(&mut self) -> &mut ModModifiedTexts {
        &mut self.text_modified
    }
    pub fn added_to_modified(&mut self, base: &GameData) {
        self.text_modified.extend(self.text_added.drain().map(|(key, value)| {
            let base = base.get(&key).expect("Attempt to modify non-added entry");
            let value = super::diff::diff(base.to_map(), value.to_map());
            (key, value)
        }))
    }
}