use super::super::{BTreeMappable, BTreePatchable, Loadable};
use crate::bundler::{
    diff::{DataMap, Patch},
    game_data::BTreeMapExt,
    loader::utils::{collect_paths, has_ext},
    ModFileChange,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StringsTable(HashMap<String, LanguageTable>);

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LanguageTable(HashMap<String, String>);

impl BTreeMappable for StringsTable {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (key, table) in &self.0 {
            out.extend_prefixed(key, table.to_map());
        }
        out
    }
}
impl BTreePatchable for StringsTable {
    fn merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Vec<ModFileChange>) {
        todo!()
    }
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        todo!()
    }
}
impl Loadable for StringsTable {
    fn prepare_list(root_path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        collect_paths(&root_path.join("localization"), |path| {
            Ok(has_ext(path, "xml"))
        })
    }
    fn load_raw(path: &std::path::Path) -> std::io::Result<Self> {
        let mut out = HashMap::new();

        let xml = std::fs::read_to_string(path)?;
        let document = roxmltree::Document::parse(&xml).expect("Malformed localization XML");
        let root = document.root_element();
        debug_assert_eq!(root.tag_name().name(), "root");
        for child in root.children() {
            if !child.is_element() {
                continue;
            }
            debug_assert_eq!(child.tag_name().name(), "language");
            let language = child.attribute("id").expect("Language ID not found");
            let mut table = HashMap::new();
            for item in child.children() {
                if !item.is_element() {
                    continue;
                }
                debug_assert_eq!(item.tag_name().name(), "entry");
                let key = item.attribute("id").expect("Entry ID not found");
                let value = item.text().unwrap_or("");
                table.insert(key.into(), value.into());
            }
            out.insert(language.into(), LanguageTable(table));
        }

        Ok(Self(out))
    }
}

impl BTreeMappable for LanguageTable {
    fn to_map(&self) -> DataMap {
        self.0
            .clone()
            .into_iter()
            .map(|(key, value)| (vec![key], value.into()))
            .collect()
    }
}
