use super::{
    super::{
        diff::{DataMap, Patch},
        ExtractionError, ModFileChange,
    },
    GameData, GameDataItem, GameDataValue, StructuredItem,
};
use crate::bundler::{diff::Conflicts, loader::utils::rel_path};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub trait BTreeLinkedMappable: Sized + Clone {
    fn from_iter(_: impl IntoIterator<Item = String>) -> Self;
    fn list(&self) -> &[String];
    fn linked_map(&self) -> BTreeMap<Option<String>, Option<&str>> {
        let mut cur: Option<&String> = None;
        let mut map = BTreeMap::new();
        for item in self.list() {
            map.insert(cur.cloned(), Some(item.as_str()));
            cur = Some(item);
        }
        map.insert(cur.cloned(), None);
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

pub trait BTreeSetable: BTreeLinkedMappable {
    fn to_set(&self) -> DataMap {
        self.list()
            .iter()
            .cloned()
            .map(|s| (vec![s.clone()], s.into()))
            .collect()
    }
}
impl<T: BTreeLinkedMappable> BTreeSetable for T {}

pub trait BTreeMappable: Sized + Clone {
    fn to_map(&self) -> DataMap;
}
pub trait BTreePatchable: Sized + Clone {
    fn merge_patches(
        &self,
        sink: &mut cursive::CbSink,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> Patch {
        let (merged, unmerged) = self.try_merge_patches(patches);
        let resolved = self.ask_for_resolve(sink, unmerged);
        let (merged, unmerged) = self.try_merge_patches(vec![
            ("merged".into(), merged),
            ("resolved".into(), resolved),
        ]);
        debug_assert!(unmerged.is_empty());
        merged
    }

    fn try_merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Conflicts);
    fn ask_for_resolve(&self, sink: &mut cursive::CbSink, conflicts: Conflicts) -> Patch;
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()>; // TODO error!
}

impl<T: BTreeLinkedMappable> BTreeMappable for T {
    fn to_map(&self) -> DataMap {
        self.linked_map()
            .into_iter()
            .map(|(key, value)| {
                (
                    key.into_iter().collect(),
                    GameDataValue::Next(value.map(|s| s.into())),
                )
            })
            .collect()
    }
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
        Self::prepare_list(root_path)
            .map_err(crate::io_to_extraction!(root_path))?
            .into_iter()
            .map(move |full_path| {
                let path = rel_path(root_path, &full_path)
                    .map_err(crate::io_to_extraction!(&full_path))?;
                $on_load(path.to_string_lossy().to_string());
                let data =
                    Self::load_raw(&full_path).map_err(crate::io_to_extraction!(&full_path))?;
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
            .map(|res| res.map(|(key, value)| (key, GameDataItem::Structured(value.into()))))
            .collect()
    }
}
impl<T: Loadable + Into<StructuredItem>> LoadableStructured for T {}
