mod diff;
mod error;
mod resolve;
mod deploy;

use crate::loader::GlobalData;
use cursive::{
    traits::{Finder, Nameable},
    views::{Dialog, LinearLayout, TextView},
    Cursive,
};
use diff::{
    DataNode, DataNodeContent, DataTree, DataTreeExt, DiffTreeExt, ModContent, ResultDiffTressExt,
};
use error::{DeploymentError, ExtractionError};
use log::*;
use std::{
    fs::read_dir,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("Background thread panicked, stopping: {0}")]
struct PanicError(String);

pub fn bundle(cursive: &mut Cursive) {
    let global_data: GlobalData = cursive.take_user_data().expect("No data was set");

    crate::screen(
        cursive,
        Dialog::around(
            LinearLayout::vertical()
                // Space added so that the view is always rendered, even when this is not specified.
                .child(TextView::new(" ").with_name("Loading part"))
                .child(TextView::new(" ").with_name("Loading filename")),
        )
        .title("Loading vanilla game data...")
        .with_name("Loading dialog"),
    );
    info!("Bundling progress dialog shown");

    let on_file_read = cursive.cb_sink().clone();
    let mut on_error = on_file_read.clone();
    std::thread::spawn(move || {
        info!("Starting background thread");
        let thread = std::thread::spawn(|| {
            let mut on_file_read = on_file_read;
            if let Err(err) = do_bundle(&mut on_file_read, global_data) {
                crate::run_update(&mut on_file_read, move |cursive| {
                    crate::error(cursive, &err);
                });
                std::thread::yield_now(); // to let cursive run update immediately
            };
        });
        info!("Waiting on the background thread");
        if let Err(panic_info) = thread.join() {
            let msg = match panic_info.downcast_ref::<&'static str>() {
                Some(s) => *s,
                None => match panic_info.downcast_ref::<String>() {
                    Some(s) => &s[..],
                    None => "Box<Any>",
                },
            }
            .to_string();
            crate::run_update(&mut on_error, move |cursive| {
                crate::error(cursive, &PanicError(msg));
            });
        } else {
            info!("Background thread exited successfully");
        }
    });
}

fn do_bundle(
    on_file_read: &mut cursive::CbSink,
    global_data: GlobalData,
) -> Result<(), error::BundlerError> {
    let path = crate::paths::game(&global_data.base_path);
    info!("Extracting data from game directory");
    let mut original_data = extract_data(on_file_read, &path, &path, true)?;
    info!("Vanilla game data extracted");

    crate::run_update(on_file_read, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Loading DLC data...");
        });
    });

    info!("Extracting DLC data");
    let dlc_path = path.join("dlc");
    for entry in read_dir(&dlc_path).map_err(ExtractionError::from_io(&dlc_path))? {
        let entry = entry.map_err(ExtractionError::from_io(&dlc_path))?;
        let path = entry.path();
        if entry
            .metadata()
            .map_err(ExtractionError::from_io(&path))?
            .is_dir()
        {
            info!("Reading DLC: {:?}", path);
            let dlc_dir_name = path
                .file_name()
                .map(std::ffi::OsStr::to_string_lossy)
                .unwrap_or_else(|| {
                    warn!("No filename in DLC directory path - this must be a bug");
                    "<INVALID>".into()
                })
                .to_string();
            crate::run_update(on_file_read, |cursive| {
                cursive
                    .call_on_name("Loading part", |text: &mut TextView| {
                        text.set_content(dlc_dir_name);
                    })
                    .unwrap();
            });
            original_data.extend(extract_data(on_file_read, &path, &path, true)?);
        } else {
            warn!("Found non-directory item in DLC folder: {:?}", path);
        }
    }
    info!("DLC data extracted and merged into vanilla game");

    crate::run_update(on_file_read, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Loading workshop data...");
            dialog.call_on_name("Loading part", |text: &mut TextView| {
                text.set_content(" ");
            })
        });
    });

    info!("Reading selected mods");
    let mut for_mods_extract = on_file_read.clone();
    let mods = global_data
        .mods
        .into_iter()
        .inspect(|the_mod| info!("Reading mod: {:?}", the_mod))
        .filter(|the_mod| the_mod.selected)
        .map(|the_mod| {
            info!("Extracting data from selected mod: {}", the_mod.name());
            extract_mod(&mut for_mods_extract, the_mod, &original_data)
        });

    let (merged, conflicts) = mods.try_merge(Some(on_file_read))?;
    info!("Merged mods data, got {} conflicts", conflicts.len());

    let resolved = resolve::resolve(on_file_read, conflicts);
    let merged = resolve::merge_resolved(merged, resolved);

    info!("Applying patches");
    let modded = merged.apply_to(original_data);
    info!("Deploying generated mod to the \"mods\" directory");
    
    let mod_path = path.join("mods/generated_bundle");
    deploy(&mod_path, modded)?;

    crate::run_update(on_file_read, |cursive| {
        crate::screen(cursive, Dialog::around(TextView::new("Bundle ready!")));
    });
    Ok(())
}

fn deploy(mod_path: &Path, bundle: DataTree) -> Result<(), DeploymentError> {
    info!("Mod is being deployed to {:?}", mod_path);
    std::fs::create_dir(mod_path).map_err(DeploymentError::from_io(&mod_path))?;

    let project_xml_path = mod_path.join("project.xml");
    std::fs::write(
        &project_xml_path,
        indoc::indoc!(
            r#"
            <?xml version="1.0" encoding="utf-8"?>
            <project>
                <Title>Generated mods bundle</Title>
            </project>
            "#
        ),
    ).map_err(DeploymentError::from_io(&project_xml_path))?;
    info!("Written project.xml");

    for (path, item) in bundle {
        info!("Writing mod file to relative path {:?}", path);
        let (source, content) = item.into_parts();
        let target = mod_path.join(path);
        match content {
            DataNodeContent::Binary => {
                info!("Copying binary file from {:?}", source);
                std::fs::copy(source, &target).map(|_| {})
            }
            DataNodeContent::Text(text) => {
                info!(
                    "Writing text file, first 100 chars = \"{}\"",
                    text.chars().take(100).collect::<String>()
                );
                std::fs::write(&target, text)
            }
        }.map_err(DeploymentError::from_io(&target))?;
    }
    Ok(())
}

fn extract_mod(
    on_file_read: &mut cursive::CbSink,
    the_mod: crate::loader::Mod,
    original_data: &DataTree,
) -> Result<ModContent, ExtractionError> {
    let title = the_mod.name().to_owned();
    crate::run_update(on_file_read, move |cursive| {
        cursive.call_on_name("Loading part", |text: &mut TextView| {
            text.set_content(title);
        });
    });
    let content = extract_data(on_file_read, &the_mod.path, &the_mod.path, true)?;
    info!("Mod {}: Data successfully extracted, calculating patch", the_mod.name());
    Ok(ModContent::new(the_mod.name(), original_data.diff(content)))
}

fn extract_data(
    on_file_read: &mut cursive::CbSink,
    base_path: &Path,
    cur_path: &Path,
    root: bool,
) -> Result<DataTree, ExtractionError> {
    info!("Extracting data from: {:?}", cur_path);
    let items = read_dir(cur_path)
        .map_err(ExtractionError::from_io(cur_path))?
        .map(|entry| {
            entry.and_then(|entry| {
                entry.metadata().map(|meta| {
                    let path = entry.path();
                    (path, meta)
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(ExtractionError::from_io(cur_path))?;
    let items = items
        .into_iter()
        .map(|(item_path, meta)| {
            if meta.is_dir() {
                if item_path.file_name().and_then(std::ffi::OsStr::to_str) == Some("dlc") {
                    debug!("Skipping DLC directory");
                    Ok(vec![])
                } else {
                    debug!("Descending into child directory {:?}", item_path);
                    extract_data(on_file_read, base_path, &item_path, false)
                        .map(|data| data.into_iter().collect())
                }
            } else if root {
                debug!("Skipping file in root: {:?}", item_path);
                // Special case - don't extract anything from root folder (there is no data there)
                Ok(vec![])
            } else {
                extract_from_file(on_file_read, base_path, &item_path)
                    .map(|(path, data)| vec![(path, data)])
                    .map_err(ExtractionError::from_io(&item_path))
            }
        })
        .collect::<Result<Vec<Vec<_>>, _>>()?;
    Ok(items.into_iter().flatten().collect())
}

fn set_file_updated(on_file_read: &mut cursive::CbSink, prefix: String, path: String) {
    const LOG_PATH_LEN: usize = 120;

    crate::run_update(on_file_read, move |cursive: &mut Cursive| {
        cursive.call_on_name("Loading filename", |text: &mut TextView| {
            let mut path = path;
            let log_path: String = if path.len() < LOG_PATH_LEN {
                path.chars()
                    .chain(std::iter::repeat(' '))
                    .take(LOG_PATH_LEN)
                    .collect()
            } else {
                // https://users.rust-lang.org/t/take-last-n-characters-from-string/44638
                let len = path
                    .char_indices()
                    .rev()
                    .nth((LOG_PATH_LEN - 3) - 1)
                    .map_or(0, |(idx, _)| idx);
                let _ = path.drain(0..len);
                format!("...{}", path)
            };
            text.set_content(format!("{}: <ROOT>/{}", prefix, log_path));
        });
    });
}

fn extract_from_file(
    on_file_read: &mut cursive::CbSink,
    base_path: &Path,
    path: &Path,
) -> std::io::Result<(PathBuf, DataNode)> {
    info!("Reading file: {:?}", path);
    let rel_path = path.strip_prefix(base_path).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Bundler reached the path outside of the working directory: {}",
                path.to_string_lossy()
            ),
        )
    })?;
    let log_path = rel_path.to_string_lossy().to_string();
    set_file_updated(on_file_read, "Reading".into(), log_path);

    let content = match path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("js") | Some("darkest") | Some("xml") | Some("json") | Some("txt") => {
            match std::fs::read_to_string(path).map(Some) {
                Ok(s) => {
                    debug!("Read successful: {:?}", path);
                    s.as_ref().map(|s| {
                        debug!(
                            "Total {} lines, {} characters",
                            s.lines().count(),
                            s.chars().count()
                        )
                    });
                    Ok(s)
                }
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                    debug!(
                        "Read unsuccessful, non-UTF8 data; asserting that {:?} is a binary file",
                        path
                    );
                    Ok(None)
                }
                err => err,
            }?
        }
        _ => {
            debug!("File extension is not in white-list (js,json,xml,txt,darkest), loading as binary");
            None
        },
    };
    Ok((rel_path.into(), DataNode::new(path, content)))
}
