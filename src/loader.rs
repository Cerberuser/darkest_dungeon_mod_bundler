use cursive::Cursive;
use log::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Project {
    #[serde(rename = "Title")]
    pub title: String,
}

#[derive(Default, Debug, Clone)]
pub struct Mod {
    pub local: bool,
    pub selected: bool,
    pub path: PathBuf,
    project: Project,
}
impl Mod {
    pub fn name(&self) -> &str {
        &self.project.title
    }
}

pub struct GlobalData {
    pub base_path: PathBuf,
    pub mods: Vec<Mod>,
}

pub fn mods_list(cursive: &mut Cursive) -> &mut [Mod] {
    &mut cursive
        .user_data::<GlobalData>()
        .expect("Mods data wasn't set")
        .mods
}

#[derive(Debug, Error)]
enum LoadModsError {
    #[error("Failed to load mods data due to IO error")]
    Io(#[from] std::io::Error),
    #[error("Broken XML in mod directory {1}")]
    XML(#[source] serde_xml_rs::Error, PathBuf),
}

fn load_mods_dir(cursive: &mut Cursive, path: PathBuf, local: bool) -> Result<Vec<Mod>, LoadModsError> {
    let dir = match std::fs::read_dir(path) {
        Ok(dir) => dir,
        Err(error) => {
            crate::error(cursive, &error);
            return Err(error.into());
        }
    };
    dir
        .map(|item| {
            item.map_err(LoadModsError::Io).and_then(|entry| {
                let path = entry.path();
                if path.is_file() {
                    return Ok(None);
                }
                let file = std::fs::File::open(path.join("project.xml"))?;
                match serde_xml_rs::from_reader::<_, Project>(file) {
                    Ok(project) => {
                        info!(
                            "Successfully parsed mod \"{}\" from directory {}",
                            project.title,
                            path.to_string_lossy()
                        );
                        Ok(Some(Mod {
                            local,
                            selected: false,
                            path,
                            project,
                        }))
                    }
                    Err(error) => Err(LoadModsError::XML(error, path)),
                }
            })
        })
        .filter_map(Result::transpose)
        .collect()
}

pub fn load_path(cursive: &mut Cursive, base_path: &str) {
    info!("Loading Steam library from path: {}", base_path);
    let base_path = base_path.into();

    // First, the Workshop mods...
    let path = crate::paths::workshop(&base_path);
    let mut mods = match load_mods_dir(cursive, path, false) {
        Ok(mods) => mods,
        Err(error) => {
            crate::error(cursive, &error);
            return;
        },
    };
    // ...then, the local ones
    let path = crate::paths::game(&base_path).join("mods");
    match load_mods_dir(cursive, path, true) {
        Ok(local_mods) => mods.extend(local_mods),
        Err(error) => {
            crate::error(cursive, &error);
            return;
        },
    };

    cursive.set_user_data(GlobalData { base_path, mods });
    crate::select::render_lists(cursive);
}
