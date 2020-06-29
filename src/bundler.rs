mod deploy;
mod diff;
mod error;
mod game_data;
mod loader;
mod resolve;

use crate::loader::GlobalData;
use cursive::{
    traits::{Finder, Nameable},
    views::{Dialog, LinearLayout, TextView},
    Cursive,
};
use diff::{
    DataMap, DataNode, DataTree, DataTreeExt, DiffTreeExt, LegacyModContent, Patch,
    ResultDiffTressExt,
};
use error::ExtractionError;
use game_data::{BTreeMappable, GameDataItem, StructuredItem};
use log::*;
use std::{
    collections::{BTreeMap, HashMap},
    fs::read_dir,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Clone, Debug)]
struct ModContent {
    binary: HashMap<PathBuf, PathBuf>,
    text_added: HashMap<PathBuf, StructuredItem>,
    text_modified: HashMap<PathBuf, Patch>,
}

pub struct ModFileChange {
    mod_name: String,
    change: Patch,
}

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
    info!("Extracting data from game directory");
    let path = crate::paths::game(&global_data.base_path);

    let mut on_load = on_file_read.clone();
    let on_load = move |s: String| set_file_updated(&mut on_load, "Reading", s);
    let mut data = game_data::load_data(on_load, &path)?;

    info!("Vanilla game data extracted");

    crate::run_update(on_file_read, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Loading DLC data...");
        });
    });

    let mut on_load = on_file_read.clone();
    let on_load = move |s: String| set_file_updated(&mut on_load, "Reading", s);

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
            data.extend(game_data::load_data(on_load.clone(), &path)?);
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
            load_mod(&mut for_mods_extract, the_mod, &data)
        });

    // Debugging
    for the_mod in mods {
        debug!("{:?}", the_mod?);
    }

    // let (merged, conflicts) = mods.try_merge(Some(on_file_read))?;
    // info!("Merged mods data, got {} conflicts", conflicts.len());

    // let resolved = resolve::resolve(on_file_read, conflicts);
    // let merged = resolve::merge_resolved(merged, resolved);

    // info!("Applying patches");
    // let modded = merged.apply_to(original_data);

    // crate::run_update(on_file_read, |cursive| {
    //     cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
    //         dialog.set_title("Deploying...");
    //     });
    // });

    // info!("Deploying generated mod to the \"mods\" directory");
    // let mods_path = path.join("mods");
    // deploy::deploy(on_file_read, &mods_path, modded)?;

    crate::run_update(on_file_read, |cursive| {
        crate::screen(
            cursive,
            Dialog::around(TextView::new("Bundle ready!")).button("OK", Cursive::quit),
        );
    });
    Ok(())
}

fn load_mod(
    on_file_read: &mut cursive::CbSink,
    the_mod: crate::loader::Mod,
    game_data: &game_data::GameData,
) -> Result<ModContent, ExtractionError> {
    let title = the_mod.name().to_owned();
    crate::run_update(on_file_read, move |cursive| {
        cursive.call_on_name("Loading part", |text: &mut TextView| {
            text.set_content(title);
        });
    });

    let mut on_load = on_file_read.clone();
    let on_load = move |s: String| set_file_updated(&mut on_load, "Reading", s);
    let content = game_data::load_data(on_load, &the_mod.path)?;
    info!(
        "Mod {}: Data successfully extracted, calculating patch",
        the_mod.name()
    );

    let mut orig_entries = game_data.iter();
    let mut mod_entries = content.into_iter();

    let mut binary = HashMap::new();
    let mut text_added = HashMap::new();
    let mut text_modified = HashMap::new();

    let mut orig_entry = orig_entries.next();
    let mut mod_entry = mod_entries.next();
    loop {
        if let Some((path, entry)) = mod_entry {
            mod_entry = mod_entries.next();
            let orig = loop {
                if let Some((old_path, old_entry)) = &orig_entry {
                    if old_path > &&path {
                        break None;
                    }
                    if old_path == &&path {
                        break Some(old_entry);
                    }
                    orig_entry = orig_entries.next();
                } else {
                    break None;
                }
            };
            match orig {
                Some(old_entry) => match (old_entry, entry) {
                    (GameDataItem::Binary(_), GameDataItem::Binary(source)) => {
                        let existing = binary.insert(path, source);
                        debug_assert!(existing.is_none());
                    }
                    (GameDataItem::Structured(orig), GameDataItem::Structured(ref modded)) => {
                        let existing =
                            text_modified.insert(path, diff::diff(orig.to_map(), modded.to_map()));
                        debug_assert!(existing.is_none());
                    }
                    _ => unreachable!(),
                },
                None => match entry {
                    GameDataItem::Binary(source) => {
                        let existing = binary.insert(path, source);
                        debug_assert!(existing.is_none());
                    }
                    GameDataItem::Structured(item) => {
                        let existing = text_added.insert(path, item);
                        debug_assert!(existing.is_none());
                    }
                },
            }
        } else {
            break;
        }
    }

    Ok(ModContent {
        binary,
        text_added,
        text_modified,
    })
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

fn set_file_updated(
    on_file_read: &mut cursive::CbSink,
    prefix: impl Into<String>,
    path: impl Into<String>,
) {
    const LOG_PATH_LEN: usize = 120;

    let prefix = prefix.into();
    let path = path.into();

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
    let log_path = rel_path.to_string_lossy();
    set_file_updated(on_file_read, "Reading", log_path);

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
            debug!(
                "File extension is not in white-list (js,json,xml,txt,darkest), loading as binary"
            );
            None
        }
    };
    Ok((rel_path.into(), DataNode::new(path, content)))
}
