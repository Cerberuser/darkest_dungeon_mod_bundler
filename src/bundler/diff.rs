use super::game_data::GameDataValue;

// use log::*;
use std::collections::BTreeMap;

pub type DataMap = BTreeMap<Vec<String>, GameDataValue>;

#[derive(Clone, Debug, PartialEq)]
pub enum ItemChange {
    Set(GameDataValue),
    Removed,
}
impl ItemChange {
    pub fn unwrap_set(self) -> GameDataValue {
        match self {
            ItemChange::Set(val) => val,
            ItemChange::Removed => panic!("Unexpected removal change"),
        }
    }
    pub fn into_option(self) -> Option<GameDataValue> {
        match self {
            ItemChange::Set(val) => Some(val),
            ItemChange::Removed => None,
        }
    }
}

pub type Patch = BTreeMap<Vec<String>, ItemChange>;
pub type Conflicts = BTreeMap<Vec<String>, Vec<(String, ItemChange)>>;

pub fn diff(original: DataMap, patched: DataMap) -> Patch {
    // debug!("Calculating diff");
    // debug!("Original: {:?}", original);
    // debug!("Patched: {:?}", patched);
    let mut out = Patch::new();
    let mut original = original.into_iter();

    let mut orig_item = original.next();
    for (path, entry) in patched {
        // debug!("Using patched item: {:?} -> {:?}", path, entry);
        loop {
            if let Some((old_path, old_entry)) = &orig_item {
                if old_path > &path {
                    out.insert(path, ItemChange::Set(entry));
                    break;
                }
                if old_path == &path {
                    if old_entry != &entry {
                        out.insert(path, ItemChange::Set(entry));
                    }
                    orig_item = original.next();
                    break;
                }
                out.insert(old_path.clone(), ItemChange::Removed);
                orig_item = original.next();
            } else {
                // Original entries are finished, but not the patched ones
                out.insert(path, ItemChange::Set(entry));
                break;
            }
        }
    }
    out
}
