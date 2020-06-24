use std::collections::BTreeMap;

mod darkest;
mod json;
mod localization;

trait MapPath: Ord + Eq {}

trait BTreeMappable: Sized {
    type Key: MapPath;
    type Value;

    fn map(&self) -> BTreeMap<Self::Key, &Self::Value>;
    fn map_mut(&mut self) -> BTreeMap<Self::Key, &mut Self::Value>;
    fn clone_with(&self, _: impl FnOnce(&mut BTreeMap<Self::Key, Self::Value>)) -> Self;
}
