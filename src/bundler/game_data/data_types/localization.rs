use super::super::{BTreeMappable, BTreePatchable, Loadable};
use crate::bundler::{
    diff::{DataMap, Patch},
    game_data::BTreeMapExt,
    loader::utils::{collect_paths, has_ext},
    ModFileChange,
};
use log::*;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap, io::Read};

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
        for patch in patches {
            debug!("{:?}", patch);
        }
        todo!()
    }
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        debug!("{:?}", patch);
        todo!()
    }
}
impl Loadable for StringsTable {
    fn prepare_list(root_path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        let path = root_path.join("localization");
        if path.exists() {
            collect_paths(&path, |path| Ok(has_ext(path, "xml")))
        } else {
            Ok(vec![])
        }
    }
    fn load_raw(path: &std::path::Path) -> std::io::Result<Self> {
        let mut out = HashMap::new();

        let mut xml_raw = vec![];
        std::fs::File::open(path)?.read_to_end(&mut xml_raw)?;
        // <HACK> Workaround: localization is sometimes invalid UTF-8
        let mut xml = match String::from_utf8_lossy(&xml_raw) {
            Cow::Borrowed(s) => String::from(s),
            Cow::Owned(s) => {
                warn!("Got some invalid UTF-8; performed lossy conversion");
                debug!("Context:");
                for capture in regex::Regex::new(&format!(
                    "(.{{0,10}}){}(.{{0, 10}})",
                    std::char::REPLACEMENT_CHARACTER
                ))
                .unwrap()
                .captures_iter(&s)
                {
                    debug!(
                        "...{}{}{}...",
                        &capture[1],
                        std::char::REPLACEMENT_CHARACTER,
                        &capture[2]
                    );
                }
                s
            }
        };
        // <HACK> Workaround: some localization files contain too big (non-existing) XML version.
        let decl = xml.lines().next().unwrap();
        let version = regex::Regex::new(r#"<?xml version="(.*?)"(.*)>"#)
            .unwrap()
            .captures(decl);
        if let Some(version) = version {
            let version = &version[1];
            if version > "1.1" {
                warn!("Got too large XML version number; replacing declaration line");
                debug!("Original declaration line: {}", decl);
                debug!("Original version: {}", version);
                xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#)
                    + xml.splitn(2, '\n').nth(1).unwrap();
            }
        }
        // <HACK> Workaround: some localization files contain invalid comments.
        xml = regex::Regex::new("<!---(.*?)--->")
            .unwrap()
            .replace_all(&xml, |cap: &regex::Captures| {
                warn!("Found invalid comment: {}", &cap[0]);
                "".to_string()
            })
            .into();
        let document = roxmltree::Document::parse(&xml)
            .unwrap_or_else(|err| panic!("Malformed localization XML {:?}: {:?}", path, err));
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
