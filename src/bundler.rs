mod diff;
mod resolve;

use crate::loader::GlobalData;
use cursive::{
    traits::{Finder, Nameable},
    views::{Dialog, LinearLayout, TextView},
    Cursive,
};
use diff::{
    DataNode, DataNodeContent, DataTree, DataTreeExt, DiffTreeExt, ModContent, ResultDiffTressExt,
};
use log::*;
use std::{
    fs::read_dir,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
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
    debug!("Bundling progress dialog shown");

    let on_file_read = cursive.cb_sink().clone();
    let mut on_error = on_file_read.clone();
    std::thread::spawn(move || {
        debug!("Starting background thread");
        let thread = std::thread::spawn(|| {
            let mut on_file_read = on_file_read;
            if let Err(err) = do_bundle(&mut on_file_read, global_data) {
                crate::run_update(&mut on_file_read, move |cursive| {
                    crate::error(cursive, &err);
                });
            };
        });
        debug!("Waiting on the background thread");
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
            debug!("Background thread exited successfully");
        }
    });
}

fn do_bundle(on_file_read: &mut cursive::CbSink, global_data: GlobalData) -> std::io::Result<()> {
    let path = crate::paths::game(&global_data.base_path);
    debug!("Extracting data from game directory");
    let mut original_data = extract_data(on_file_read, &path, &path, true)?;
    debug!("Vanilla game data extracted");

    crate::run_update(on_file_read, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Loading DLC data...");
        });
    });

    debug!("Extracting DLC data");
    for entry in read_dir(path.join("dlc"))? {
        let entry = entry?;
        if entry.metadata()?.is_dir() {
            let path = entry.path();
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
            warn!(
                "Found non-directory item in DLC folder: {}",
                entry.path().to_string_lossy()
            );
        }
    }
    debug!("DLC data extracted and merged into vanilla game");

    crate::run_update(on_file_read, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Loading workshop data...");
            dialog.call_on_name("Loading part", |text: &mut TextView| {
                text.set_content(" ");
            })
        });
    });

    debug!("Reading selected mods");
    let mut for_mods_extract = on_file_read.clone();
    let mods = global_data
        .mods
        .into_iter()
        .inspect(|the_mod| debug!("Reading mod: {:?}", the_mod))
        .filter(|the_mod| the_mod.selected)
        .map(|the_mod| {
            info!("Extracting data from selected mod: {}", the_mod.name());
            extract_mod(&mut for_mods_extract, the_mod, &original_data)
        });

    let (merged, conflicts) = mods.try_merge(Some(on_file_read))?;
    info!("Merged mods data, got {} conflicts", conflicts.len());

    let resolved = resolve::resolve(on_file_read, conflicts);
    let merged = resolve::merge_resolved(merged, resolved);

    debug!("Applying patches");
    let modded = merged.apply_to(original_data);
    debug!("Deploying generated mod to the \"mods\" directory");
    deploy(&path, modded)?;

    crate::run_update(on_file_read, |cursive| {
        crate::screen(cursive, Dialog::around(TextView::new("Bundle ready!")));
    });
    Ok(())
}

fn deploy(game_path: &Path, bundle: DataTree) -> std::io::Result<()> {
    let base = game_path.join("mods/generated_bundle");
    info!("Mod is being deployed to {:?}", base);

    std::fs::write(
        base.join("project.xml"),
        indoc::indoc!(
            r#"
            <?xml version="1.0" encoding="utf-8"?>
            <project>
                <Title>Generated mods bundle</Title>
            </project>
            "#
        ),
    )?;
    debug!("Written project.xml");

    for (path, item) in bundle {
        info!("Writing mod file to relative path {:?}", path);
        let (source, content) = item.into_parts();
        let target = base.join(path);
        match content {
            DataNodeContent::Binary => {
                debug!("Copying binary file from {:?}", source);
                std::fs::copy(source, target)?;
            }
            DataNodeContent::Text(text) => {
                debug!(
                    "Writing text file, first 100 chars = \"{}\"",
                    text.chars().take(100).collect::<String>()
                );
                std::fs::write(target, text)?;
            }
        }
    }
    Ok(())
}

fn extract_mod(
    on_file_read: &mut cursive::CbSink,
    the_mod: crate::loader::Mod,
    original_data: &DataTree,
) -> std::io::Result<ModContent> {
    let title = the_mod.name().to_owned();
    crate::run_update(on_file_read, move |cursive| {
        cursive.call_on_name("Loading part", |text: &mut TextView| {
            text.set_content(title);
        });
    });
    let content = extract_data(on_file_read, &the_mod.path, &the_mod.path, true)?;
    debug!("Data successfully extracted, calculating patch");
    Ok(ModContent::new(the_mod.name(), original_data.diff(content)))
}

fn extract_data(
    on_file_read: &mut cursive::CbSink,
    base_path: &Path,
    cur_path: &Path,
    root: bool,
) -> std::io::Result<DataTree> {
    info!("Extracting data from: {:?}", cur_path);
    let items = read_dir(cur_path)?
        .map(|entry| {
            entry.and_then(|entry| {
                entry.metadata().map(|meta| {
                    let path = entry.path();
                    debug!("Collecting children: {:?}", path);
                    (path, meta)
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let items = items
        .into_iter()
        .map(|(item_path, meta)| {
            if meta.is_dir() {
                debug!("Extracting data from child directory {:?}", item_path);
                if item_path.file_name().and_then(std::ffi::OsStr::to_str) == Some("dlc") {
                    debug!("Skipping DLC directory");
                    Ok(vec![])
                } else {
                    debug!("Descending into child");
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
            }
        })
        .collect::<Result<Vec<Vec<_>>, _>>()?;
    Ok(items.into_iter().flatten().collect())
}

fn set_file_updated(on_file_read: &mut cursive::CbSink, prefix: String, path: String) {
    const LOG_PATH_LEN: usize = 120;

    crate::run_update(on_file_read, move |cursive: &mut Cursive| {
        cursive.call_on_name("Loading filename", |text: &mut TextView| {
            debug!(
                "Bundler is reading path {}, setting it in progress window",
                path
            );
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
    debug!("Reading file: {:?}", path);
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
        Some("js") | Some("darkest") | Some("xml") | Some("json") => {
            match std::fs::read_to_string(path).map(Some) {
                Ok(s) => {
                    info!("Read successful: {:?}", path);
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
                    info!(
                        "Read unsuccessful, non-UTF8 data; asserting that {:?} is a binary file",
                        path
                    );
                    Ok(None)
                }
                err => err,
            }?
        }
        _ => None,
    };
    Ok((rel_path.into(), DataNode::new(path, content)))
}
