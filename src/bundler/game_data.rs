use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

pub mod data_types;
pub mod file_types;

mod traits;
pub use traits::*;

use super::{
    diff::{DataMap, Patch},
    ModFileChange,
};

macro_rules! game_data_value {
    ($($id:ident($ty:ty)),+ $(,)?) => {
        #[derive(Serialize, Deserialize, Clone, Debug)]
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
        pub enum StructuredItem { $($ty(data_types::$ty)),+ }
        impl BTreeMappable for StructuredItem {
            fn to_map(&self) -> DataMap {
                match self {
                    $(Self::$ty(value) => value.to_map()),+
                }
            }
        }
        impl BTreePatchable for StructuredItem {
            fn merge_patches(&self, patches: impl IntoIterator<Item = ModFileChange>) -> (Patch, Vec<ModFileChange>) {
                match self {
                    $(Self::$ty(value) => value.merge_patches(patches)),+
                }
            }
            fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
                match self {
                    $(Self::$ty(value) => value.apply_patch(patch)),+
                }
            }
        }
        $(
            impl From<data_types::$ty> for StructuredItem {
                fn from(item: data_types::$ty) -> Self {
                    Self::$ty(item)
                }
            }
        )+
    };
}

structured! {
    HeroInfo,
}

#[derive(Clone, Debug)]
pub enum GameDataItem {
    Binary(PathBuf),
    Structured(StructuredItem),
}

pub type GameData = BTreeMap<PathBuf, GameDataItem>;

pub fn load_data(
    on_load: impl FnMut(String) + Clone,
    root_path: &Path,
) -> std::io::Result<GameData> {
    let mut data = GameData::new();

    macro_rules! load {
        ($($ty:ident),+ $(,)?) => {
            $(
                data.extend(data_types::$ty::load(on_load.clone(), root_path)?);
            )+
        };
    }
    load! {
        ActivityLogImage,
        AudioData,
        CampaignData,
        HeroInfo,
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
