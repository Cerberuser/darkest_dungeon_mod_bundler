use super::{
    super::{
        diff::{DataMap, Patch},
        ModFileChange,
        ExtractionError,
    },
    GameData, GameDataItem, GameDataValue, StructuredItem,
};
use crate::bundler::loader::utils::rel_path;
use std::{
    collections::{BTreeMap, HashMap},
    io::Result as IoResult,
    path::{Path, PathBuf},
};
use log::*;

pub trait BTreeLinkedMappable: Sized + Clone {
    fn from_iter(_: impl IntoIterator<Item = String>) -> Self;
    fn list(&self) -> &[String];
    fn linked_map(&self) -> BTreeMap<String, Option<&str>> {
        let mut cur = None::<&String>;
        let mut map = BTreeMap::new();
        for item in self.list() {
            if let Some(prev) = cur {
                map.insert(prev.into(), Some(item.as_str()));
            }
            cur = Some(item);
        }
        if let Some(last) = cur {
            map.insert(last.clone(), None);
        }
        map
    }
}

impl BTreeLinkedMappable for Vec<String> {
    fn from_iter(iter: impl IntoIterator<Item = String>) -> Self {
        iter.into_iter().collect()
    }
    fn list(&self) -> &[String] {
        &*self
    }
}

pub trait BTreeMappable: Sized + Clone {
    fn to_map(&self) -> DataMap;
}
pub trait BTreePatchable: Sized + Clone {
    fn merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Vec<ModFileChange>);
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()>; // TODO error!
}

impl<T: BTreeLinkedMappable> BTreeMappable for T {
    fn to_map(&self) -> DataMap {
        self.linked_map()
            .into_iter()
            .map(|(key, value)| (vec![key], GameDataValue::Next(value.map(|s| s.into()))))
            .collect()
    }
}
impl<T: BTreeLinkedMappable> BTreePatchable for T {
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

pub fn btree_vec(items: &[impl BTreeMappable]) -> DataMap {
    let mut map = DataMap::new();
    for item in items {
        map.extend(item.to_map());
    }
    map
}

// Supply trait, to simplify common operation
pub trait BTreeMapExt<Item>: Extend<(Vec<String>, Item)> {
    fn extend_prefixed(
        &mut self,
        prefix: &str,
        data: impl IntoIterator<Item = (Vec<String>, Item)>,
    ) {
        self.extend(data.into_iter().map(|(mut path, item)| {
            path.insert(0, prefix.into());
            (path, item)
        }));
    }
}
impl<V> BTreeMapExt<V> for BTreeMap<Vec<String>, V> {}

pub trait Loadable: Sized {
    fn prepare_list(root_path: &Path) -> std::io::Result<Vec<PathBuf>>;
    fn load_raw(path: &Path) -> std::io::Result<Self>;
}

// This is a macro and not a function, so that we don't have
// to struggle with all the lifetime specifications on function boundaries.
macro_rules! load {
    ($on_load:expr, $root_path:expr) => {{
        let root_path = $root_path.as_ref();
        Self::prepare_list(root_path).map_err(ExtractionError::from_io(root_path))?
            .into_iter()
            .map(move |full_path| {
                debug!("Starting loading from path {:?}", full_path);
                let path = rel_path(root_path, &full_path).map_err(ExtractionError::from_io(&full_path))?;
                debug!("Calculated relative path: {:?}", path);
                $on_load(path.to_string_lossy().to_string());
                let data = Self::load_raw(&full_path).map_err(ExtractionError::from_io(&full_path))?;
                Ok((path, data))
            })
    }};
}

pub trait Binary {
    fn into_path(self) -> PathBuf;
}
pub trait LoadableBinary: Loadable + Binary {
    fn load(
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> Result<GameData, ExtractionError> {
        load!(on_load, root_path)
            .map(|res| res.map(|(key, value)| (key, GameDataItem::Binary(value.into_path()))))
            .collect()
    }
}
impl<T: Loadable + Binary> LoadableBinary for T {}
pub trait LoadableStructured: Loadable + Into<StructuredItem> {
    fn load(
        mut on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> Result<GameData, ExtractionError> {
        load!(on_load, root_path)
            .map(|res| res.map(|(key, value)|(key, GameDataItem::Structured(value.into()))))
            .collect()
    }
}
impl<T: Loadable + Into<StructuredItem>> LoadableStructured for T {}
