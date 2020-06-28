use super::{
    super::{
        diff::{DataMap, Patch},
        ModFileChange,
    },
    GameData, GameDataItem, GameDataValue, StructuredItem,
};
use std::{
    collections::{BTreeMap, HashMap},
    io::Result as IoResult,
    path::{Path, PathBuf},
};

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
    fn load_raw(
        on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<HashMap<PathBuf, Self>>;
}
pub trait Binary {
    fn into_path(self) -> PathBuf;
}
pub trait LoadableBinary: Loadable + Binary {
    fn load(
        on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<GameData> {
        Ok(Self::load_raw(on_load, root_path)?
            .into_iter()
            .map(|(key, value)| (key, GameDataItem::Binary(value.into_path())))
            .collect())
    }
}
impl<T: Loadable + Binary> LoadableBinary for T {}
pub trait LoadableStructured: Loadable + Into<StructuredItem> {
    fn load(
        on_load: impl FnMut(String) + Clone,
        root_path: impl AsRef<Path>,
    ) -> IoResult<GameData> {
        Ok(Self::load_raw(on_load, root_path)?
            .into_iter()
            .map(|(key, value)| (key, GameDataItem::Structured(value.into())))
            .collect())
    }
}
impl<T: Loadable + Into<StructuredItem>> LoadableStructured for T {}
