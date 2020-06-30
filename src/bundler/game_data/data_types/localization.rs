use super::super::{BTreeMappable, BTreePatchable, Loadable};
use crate::bundler::{
    diff::{Conflicts, DataMap, ItemChange, Patch},
    game_data::{BTreeMapExt, GameDataValue},
    loader::utils::{collect_paths, has_ext},
    ModFileChange,
};
use crossbeam_channel::bounded;
use cursive::{
    traits::{Nameable, Resizable},
    views::{Button, Dialog, LinearLayout, Panel, TextArea, TextView},
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
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        for (path, value) in patch {
            if path.len() != 2 {
                return Err(());
            }
            let language = path.get(0).unwrap();
            let entry_key = path.get(1).unwrap();
            let lang_table = &mut self.0.get_mut(language).ok_or(())?.0;
            match value {
                ItemChange::Set(value) => match value {
                    GameDataValue::String(value) => lang_table.insert(entry_key.clone(), value),
                    _ => return Err(()),
                },
                ItemChange::Removed => lang_table.remove(entry_key),
            };
        }
        Ok(())
    }

    fn try_merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Conflicts) {
        let mut merged = Patch::new();
        let mut unmerged = Conflicts::new();

        // TODO - this is almost the same as `regroup` in `resolve` module
        let mut changes = HashMap::new();
        for (mod_name, mod_changes) in patches {
            for (path, item) in mod_changes {
                // False positive from clippy - https://github.com/rust-lang/rust-clippy/issues/5693
                #[allow(clippy::or_fun_call)]
                changes
                    .entry(path)
                    .or_insert(vec![])
                    .push((mod_name.clone(), item));
            }
        }
        for (path, changes) in changes {
            debug_assert!(!changes.is_empty());
            if changes.len() == 1 {
                merged.insert(path, changes.into_iter().next().unwrap().1);
            } else {
                for change in changes {
                    unmerged.entry(path.clone()).or_default().push(change)
                }
            }
        }
        (merged, unmerged)
    }

    fn ask_for_resolve(&self, sink: &mut cursive::CbSink, conflicts: Conflicts) -> Patch {
        let mut patch = Patch::new();
        for (path, conflict) in conflicts {
            debug_assert!(path.len() == 2);
            let language = path.get(0).unwrap().clone();
            let entry_key = path.get(1).unwrap().clone();

            let (sender, receiver) = bounded(0);
            crate::run_update(sink, move |cursive| {
                let mut layout = LinearLayout::vertical();
                conflict.into_iter().for_each(|(name, line)| {
                    layout.add_child(
                        LinearLayout::horizontal()
                            .child(
                                Panel::new(match &line {
                                    ItemChange::Set(GameDataValue::String(value)) => {
                                        TextView::new(value.clone())
                                    }
                                    ItemChange::Removed => TextView::new("<Removed>"),
                                    otherwise => panic!(
                                        "Unexpected value in localization table: {:?}",
                                        otherwise
                                    ),
                                })
                                .title(name),
                            )
                            .child(Button::new("Move to input", move |cursive| {
                                cursive.call_on_name("Line resolve edit", |edit: &mut TextArea| {
                                    edit.set_content(match &line {
                                        ItemChange::Set(GameDataValue::String(value)) => {
                                            value.clone()
                                        }
                                        ItemChange::Removed => "".into(),
                                        otherwise => panic!(
                                            "Unexpected value in localization table: {:?}",
                                            otherwise
                                        ),
                                    })
                                });
                            })),
                    )
                });
                let resolve_sender = sender.clone();
                crate::push_screen(
                    cursive,
                    Dialog::around(
                        layout.child(TextArea::new().with_name("Line resolve edit").full_width()),
                    )
                    .title(format!(
                        "Resolving entry: language = {}, entry = {}",
                        language, entry_key,
                    ))
                    .button("Resolve", move |cursive| {
                        let value = cursive
                            .call_on_name("Line resolve edit", |edit: &mut TextArea| {
                                edit.get_content().to_owned()
                            })
                            .unwrap();
                        cursive.pop_layer();
                        resolve_sender.send(ItemChange::Set(value.into())).unwrap();
                    })
                    .button("Remove", move |cursive| {
                        cursive.pop_layer();
                        sender.send(ItemChange::Removed).unwrap();
                    })
                    .h_align(cursive::align::HAlign::Center),
                );
            });
            let choice = receiver
                .recv()
                .expect("Sender was dropped without sending anything");
            patch.insert(path, choice);
        }
        patch
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

        // <HACK> Workaround: localization is sometimes invalid UTF-8.
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
                        &capture[1].escape_debug(),
                        std::char::REPLACEMENT_CHARACTER,
                        &capture[2].escape_debug()
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

        // OK, hacks are ended for now, let's load
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
