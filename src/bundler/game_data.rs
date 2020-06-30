use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf}, fmt::Display,
};

pub mod data_types;
pub mod file_types;

mod traits;
pub use traits::*;

use super::{
    diff::{DataMap, Patch, Conflicts},
    ExtractionError, ModFileChange,
};
use log::debug;

macro_rules! game_data_value {
    ($($id:ident($ty:ty)),+ $(,)?) => {
        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
        #[serde(untagged)]
        pub enum GameDataValue {
            $(
                $id($ty),
            )+
        }
        $(
            impl From<$ty> for GameDataValue {
                fn from(value: $ty) -> Self {
                    Self::$id(value)
                }
            }
        )+
    };
}
game_data_value! {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    Next(Option<String>),
    Unit(()),
}

#[allow(dead_code)] // since some unwrap_* methods are yet unused
impl GameDataValue {
    pub fn unwrap_bool(self) -> bool {
        match self {
            GameDataValue::Bool(b) => b,
            otherwise => panic!("Expected bool, got {:?}", otherwise),
        }
    }
    pub fn unwrap_i32(self) -> i32 {
        match self {
            GameDataValue::Int(i) => i,
            otherwise => panic!("Expected integer, got {:?}", otherwise),
        }
    }
    pub fn unwrap_f32(self) -> f32 {
        match self {
            GameDataValue::Float(f) => f,
            otherwise => panic!("Expected float, got {:?}", otherwise),
        }
    }
    pub fn unwrap_string(self) -> String {
        match self {
            GameDataValue::String(s) => s,
            otherwise => panic!("Expected string, got {:?}", otherwise),
        }
    }
    pub fn unwrap_list_next(self) -> Option<String> {
        match self {
            GameDataValue::Next(s) => s,
            otherwise => panic!("Expected next value in string list, got {:?}", otherwise),
        }
    }
    pub fn unwrap_unit(self) {
        match self {
            GameDataValue::Unit(()) => (),
            otherwise => panic!("Expected unit type (a marker of key existence), got {:?}", otherwise),
        }
    }
    pub fn parse_replace(&mut self, input: &str) -> Result<(), ()> {
        match self {
            GameDataValue::Bool(b) => *b = input.parse().map_err(|_| {})?,
            GameDataValue::Int(i) => *i = input.parse().map_err(|_| {})?,
            GameDataValue::Float(f) => *f = input.parse().map_err(|_| {})?,
            GameDataValue::String(s) => *s = input.parse().map_err(|_| {})?,
            _ => panic!("Next-like and Unit-like values can't be parsed into"),
        };
        Ok(())
    }
}
impl Display for GameDataValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameDataValue::Bool(b) => b.fmt(f),
            GameDataValue::Int(i) => i.fmt(f),
            GameDataValue::Float(d) => d.fmt(f),
            GameDataValue::String(s) => s.fmt(f),
            GameDataValue::Next(Some(s)) => s.fmt(f),
            GameDataValue::Next(None) => {
                debug!("Trying to Display the GameDataValue::Next(None), outputting nothing");
                Ok(())
            }
            GameDataValue::Unit(_) => {
                debug!("Trying to Display the GameDataValue::Unit, outputting nothing");
                Ok(())
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct RestMap(HashMap<String, GameDataValue>);
impl BTreeMappable for RestMap {
    fn to_map(&self) -> super::diff::DataMap {
        self.0
            .iter()
            .map(|(key, value)| (vec![key.into()], value.clone()))
            .collect()
    }
}

macro_rules! structured {
    ($($ty:ident),+ $(,)?) => {
        #[derive(Clone, Debug)]
        pub enum StructuredItem { $($ty(Box<data_types::$ty>)),+ }
        impl BTreeMappable for StructuredItem {
            fn to_map(&self) -> DataMap {
                match self {
                    $(Self::$ty(value) => value.to_map()),+
                }
            }
        }
        impl BTreePatchable for StructuredItem {
            fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
                match self {
                    $(Self::$ty(value) => value.apply_patch(patch)),+
                }
            }
            fn try_merge_patches(
                &self,
                patches: impl IntoIterator<Item = ModFileChange>,
            ) -> (Patch, Conflicts) {
                match self {
                    $(Self::$ty(value) => value.try_merge_patches(patches)),+
                }
            }
            fn ask_for_resolve(&self, sink: &mut cursive::CbSink, patches: Conflicts) -> Patch {
                match self {
                    $(Self::$ty(value) => value.ask_for_resolve(sink, patches)),+
                }
            }
        }
        $(
            impl From<data_types::$ty> for StructuredItem {
                fn from(item: data_types::$ty) -> Self {
                    Self::$ty(Box::new(item))
                }
            }
        )+
    };
}

structured! {
    HeroInfo,
    HeroOverride,
    StringsTable,
}

#[derive(Clone, Debug)]
pub enum GameDataItem {
    Binary(PathBuf),
    Structured(StructuredItem),
}

impl BTreeMappable for GameDataItem {
    fn to_map(&self) -> DataMap {
        match self {
            GameDataItem::Binary(_) => panic!("Attempt to make a map from the binary item, probably a bug"),
            GameDataItem::Structured(item) => item.to_map(),
        }
    }
}
impl BTreePatchable for GameDataItem {
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        match self {
            GameDataItem::Binary(_) => panic!("Attempt to patch the binary item, probably a bug"),
            GameDataItem::Structured(item) => item.apply_patch(patch),
        }
    }
    fn try_merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Conflicts) {
        match self {
            GameDataItem::Binary(_) => panic!("Attempt to patch the binary item, probably a bug"),
            GameDataItem::Structured(item) => item.try_merge_patches(patches),
        }
    }
    fn ask_for_resolve(&self, sink: &mut cursive::CbSink, patches: Conflicts) -> Patch {
        match self {
            GameDataItem::Binary(_) => panic!("Attempt to patch the binary item, probably a bug"),
            GameDataItem::Structured(item) => item.ask_for_resolve(sink, patches),
        }
    }
}

pub type GameData = BTreeMap<PathBuf, GameDataItem>;

pub fn load_data(
    on_load: impl FnMut(String) + Clone,
    root_path: &Path,
) -> Result<GameData, ExtractionError> {
    let mut data = GameData::new();

    macro_rules! load {
        ($($ty:ident),+ $(,)?) => {
            $(
                data.extend(data_types::$ty::load(on_load.clone(), root_path)?);
            )+
        };
    }
    load! {
        BinaryData,
        HeroInfo,
        HeroOverride,
        HeroBinary,
        StringsTable,
    }

    Ok(data)
}

// pub fn check_unsupported(root_path: &Path) -> std::io::Result<Result<(), Vec<PathBuf>>> {
//     let mut errors = vec![];
//     for path in &[
//         "campaign/estate",
//         "campaign/heirloom_exchange",
//         "campaign/progression",
//     ] {
//         let path = root_path.join(path);
//         if path.exists() {
//             for entry in std::fs::read_dir(path)? {
//                 let entry = entry?;
//                 errors.push(entry.path().strip_prefix(&root_path).unwrap().to_path_buf());
//             }
//         }
//     }

//     Ok(if errors.is_empty() {
//         Ok(())
//     } else {
//         Err(errors)
//     })
// }
