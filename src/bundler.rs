mod deploy;
mod diff;
mod error;
mod game_data;
mod loader;
mod mod_content;
mod resolve;

use mod_content::*;

use crate::loader::GlobalData;
use cursive::{
    traits::{Finder, Nameable},
    views::{Dialog, LinearLayout, TextView},
    Cursive,
};
use diff::Patch;
use error::ExtractionError;
use game_data::{BTreeMappable, BTreePatchable, GameData, GameDataItem};
use log::*;
use std::{collections::HashMap, fs::read_dir, path::Path};
use thiserror::Error;

pub type ModFileChange = (String, Patch);

#[derive(Debug, Error)]
#[error("Background thread panicked, stopping: {0}")]
struct PanicError(String);

pub fn bundle(cursive: &mut Cursive) {
    let mut global_data: GlobalData = cursive.take_user_data().expect("No data was set");
    global_data
        .mods
        .sort_by_key(|the_mod| the_mod.name().to_owned());

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

    info!("Extracting DLC data");
    load_dlcs(&path.join("dlc"), &mut data, on_file_read)?;
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
    let mut mods: HashMap<_, _> = global_data
        .mods
        .into_iter()
        .inspect(|the_mod| info!("Reading mod: {:?}", the_mod))
        .filter(|the_mod| the_mod.selected)
        .map(|the_mod| {
            let name = the_mod.name().to_string();
            info!("Extracting data from selected mod: {}", name);
            let loaded = load_mod(&mut for_mods_extract, the_mod, &data)?;
            Ok((name, loaded))
        })
        .collect::<Result<_, ExtractionError>>()?;

    // First, ask user to choose binary files.
    let binaries = mods
        .iter_mut()
        .map(|(name, content)| (name.clone(), content.binary_mut()))
        .collect();
    let binaries = resolve::resolve_binaries(on_file_read, binaries)?;

    // Next, ask to choose the basic file from a list of added ones
    let text_added = mods
        .iter_mut()
        .map(|(name, content)| (name.clone(), content.text_added_mut()))
        .collect();
    let text_added = resolve::resolve_added_text(on_file_read, text_added);
    // These added files are treated "as if" they were in the unmodded game
    data.extend(
        text_added
            .iter()
            .map(|(key, value)| (key.clone(), GameDataItem::Structured(value.clone()))),
    );
    // ...and everything that remains in mods should be diffed against it
    for content in mods.values_mut() {
        content.added_to_modified(&data);
    }

    // Last, we merge the changes (both original and introduced at the previous step)
    let text_modified = mods
        .iter_mut()
        .map(|(name, content)| (name.clone(), content.text_modified_mut()))
        .collect();
    let text_modified = resolve::resolve_modified_text(on_file_read, &data, text_modified);

    // Merge every changes into the single tree
    let mut mods_data: game_data::GameData = binaries
        .into_iter()
        .map(|(key, value)| (key, GameDataItem::Binary(value)))
        .collect();
    // Merge first the added files - they might not be changed later
    mods_data.extend(
        text_added
            .into_iter()
            .map(|(key, value)| (key, GameDataItem::Structured(value))),
    );
    // Then, apply the patches.
    for (path, patch) in text_modified {
        let mut file = data.remove(&path).unwrap_or_else(|| {
            panic!(
                "Attempt to modify non-existing file {:?} - possibly a bug",
                path
            )
        });
        file.apply_patch(patch)
            .unwrap_or_else(|_| panic!("Error applying patch to {:?}", path));
        mods_data.insert(path, file);
    }

    // That's it. Mod data is ready.

    crate::run_update(on_file_read, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Deploying...");
        });
    });

    info!("Deploying generated mod to the \"mods\" directory");
    let mods_path = path.join("mods");
    deploy::deploy(on_file_read, &mods_path, mods_data)?;

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
    let mut content = game_data::load_data(on_load, &the_mod.path)?;
    // <HACK> This looks like a bad practice, but I've run into mod which does exactly that
    let dlc_path = the_mod.path.join("dlc");
    if dlc_path.exists() {
        warn!("File contains DLC-mapped data; loading");
        load_dlcs(&dlc_path, &mut content, on_file_read)?;
    }
    let content = content;

    info!(
        "Mod {}: Data successfully extracted, calculating patch",
        the_mod.name()
    );

    let mut orig_entries = game_data.iter();

    let mut binary = HashMap::new();
    let mut text_added = HashMap::new();
    let mut text_modified = HashMap::new();

    let mut orig_entry = orig_entries.next();
    for (path, entry) in content {
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
    }

    Ok(ModContent::build(binary, text_added, text_modified))
}

fn load_dlcs(
    dlc_path: &Path,
    data: &mut GameData,
    sink: &mut cursive::CbSink,
) -> Result<(), ExtractionError> {
    let mut on_load = sink.clone();
    let on_load = move |s: String| set_file_updated(&mut on_load, "Reading", s);

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
            crate::run_update(sink, |cursive| {
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
    Ok(())
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
