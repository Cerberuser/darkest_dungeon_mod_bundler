use super::super::{
    btree_vec, BTreeLinkedMappable, BTreeMapExt, BTreeMappable, BTreePatchable, Binary, Loadable,
    RestMap,
};
use crate::bundler::{
    diff::DataMap,
    loader::utils::{collect_tree, ends_with, has_ext, load_json},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Result as IoResult,
    path::{Path, PathBuf},
};

pub struct AudioBank(PathBuf);
impl Binary for AudioBank {
    fn into_path(self) -> PathBuf {
        self.0
    }
}

impl Loadable for AudioBank {
    fn load_raw(
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<HashMap<PathBuf, Self>> {
        collect_tree(
            root_path.as_ref(),
            &root_path.as_ref().join("audio"),
            move |path| {
                if has_ext(path, "bank") {
                    on_load(path.to_string_lossy().into());
                    Ok(Some(AudioBank(path.to_path_buf())))
                } else {
                    Ok(None)
                }
            },
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoadOrder {
    load_order: Vec<String>,
}

impl BTreeLinkedMappable for LoadOrder {
    fn from_iter(iter: impl IntoIterator<Item = String>) -> Self {
        Self {
            load_order: iter.into_iter().collect(),
        }
    }
    fn list(&self) -> &[String] {
        &self.load_order
    }
}

impl Loadable for LoadOrder {
    fn load_raw(
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<HashMap<PathBuf, Self>> {
        collect_tree(
            root_path.as_ref(),
            &root_path.as_ref().join("audio"),
            move |path| {
                if ends_with(&path, ".load_order.json") {
                    on_load(path.to_string_lossy().into());
                    load_json(path).map(Some)
                } else {
                    Ok(None)
                }
            },
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Narration {
    filters: Vec<String>,
    entries: Vec<NarrationEntry>,
}

impl Loadable for Narration {
    fn load_raw(
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<HashMap<PathBuf, Self>> {
        collect_tree(
            root_path.as_ref(),
            &root_path.as_ref().join("audio"),
            move |path| {
                if ends_with(&path, "narration.json") {
                    on_load(path.to_string_lossy().into());
                    load_json(path).map(Some)
                } else {
                    Ok(None)
                }
            },
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NarrationEntry {
    id: String,
    audio_events: Vec<AudioEvent>,
    #[serde(flatten)]
    rest: RestMap,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioEvent {
    audio_event: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    clear_queue_tags: Vec<String>,
    #[serde(default)]
    all_tags: Vec<String>,
    #[serde(default)]
    single_tags: Vec<String>,
    #[serde(flatten)]
    rest: RestMap,
}

impl BTreePatchable for Narration {
    fn merge_patches(
        &self,
        patches: impl IntoIterator<Item = crate::bundler::ModFileChange>,
    ) -> (
        crate::bundler::diff::Patch,
        Vec<crate::bundler::ModFileChange>,
    ) {
        todo!()
    }
    fn apply_patch(&mut self, patch: crate::bundler::diff::Patch) -> Result<(), ()> {
        todo!()
    }
}

impl BTreeMappable for Narration {
    fn to_map(&self) -> DataMap {
        let mut map = DataMap::new();
        map.extend_prefixed("filters", self.filters.to_map());
        map.extend_prefixed("entries", self.entries.to_map());
        map
    }
}

impl BTreeMappable for NarrationEntry {
    fn to_map(&self) -> DataMap {
        let mut map = DataMap::new();
        map.extend_prefixed(&self.id, self.audio_events.to_map());
        map.extend_prefixed(&self.id, self.rest.to_map());
        map
    }
}

impl BTreeMappable for Vec<NarrationEntry> {
    fn to_map(&self) -> DataMap {
        btree_vec(self)
    }
}

impl BTreeMappable for AudioEvent {
    fn to_map(&self) -> DataMap {
        let mut map = DataMap::new();
        let id = &self.audio_event;
        let mut tags_map = DataMap::new();
        tags_map.extend_prefixed("tags", self.tags.to_map());
        tags_map.extend_prefixed("all_tags", self.all_tags.to_map());
        tags_map.extend_prefixed("single_tags", self.single_tags.to_map());
        map.extend_prefixed(id, tags_map);
        map.extend_prefixed(id, self.rest.to_map());
        map
    }
}

impl BTreeMappable for Vec<AudioEvent> {
    fn to_map(&self) -> DataMap {
        btree_vec(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventGuidOverrides {
    event_guid_overrides: Vec<Override>,
}

impl BTreeMappable for EventGuidOverrides {
    fn to_map(&self) -> DataMap {
        self.event_guid_overrides.to_map()
    }
}

impl BTreePatchable for EventGuidOverrides {
    fn merge_patches(
        &self,
        patches: impl IntoIterator<Item = crate::bundler::ModFileChange>,
    ) -> (
        crate::bundler::diff::Patch,
        Vec<crate::bundler::ModFileChange>,
    ) {
        todo!()
    }
    fn apply_patch(&mut self, patch: crate::bundler::diff::Patch) -> Result<(), ()> {
        todo!()
    }
}

impl Loadable for EventGuidOverrides {
    fn load_raw(
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<HashMap<PathBuf, Self>> {
        collect_tree(
            root_path.as_ref(),
            &root_path.as_ref().join("audio"),
            move |path| {
                if ends_with(&path, ".guid_overrides.json") {
                    on_load(path.to_string_lossy().into());
                    load_json(path).map(Some)
                } else {
                    Ok(None)
                }
            },
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Override {
    event_id: String,
    guid_override: String,
}

impl BTreeMappable for Override {
    fn to_map(&self) -> DataMap {
        let mut map = DataMap::new();
        map.insert(
            vec![self.event_id.clone()],
            self.guid_override.clone().into(),
        );
        map
    }
}

impl BTreeMappable for Vec<Override> {
    fn to_map(&self) -> DataMap {
        btree_vec(self)
    }
}
