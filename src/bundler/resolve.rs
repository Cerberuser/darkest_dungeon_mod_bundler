use super::{
    error::ResolveError,
    game_data::{BTreePatchable, GameData},
    mod_content::{ModAddedTexts, ModBinaries, ModModifiedTexts},
};
use crossbeam_channel::bounded;
use cursive::views::{Dialog, LinearLayout, Panel, SelectView, TextView};
use log::*;
use std::collections::HashMap;
use std::{fmt::Debug, io::Read, path::PathBuf};

pub fn resolve_binaries(
    sink: &mut cursive::CbSink,
    data: HashMap<String, &mut ModBinaries>,
) -> Result<ModBinaries, ResolveError> {
    let mut out = ModBinaries::new();
    for (path, changes) in regroup(data) {
        debug_assert!(!changes.is_empty());

        if changes.len() == 1 {
            out.insert(path, changes.into_iter().next().unwrap().1);
        } else {
            // Check if the changed items are equivalent.
            let mut files = changes
                .iter()
                .map(|(_, path)| {
                    let file = std::io::BufReader::new(
                        std::fs::File::open(path).map_err(ResolveError::from_io(path))?,
                    )
                    .bytes();
                    Ok((path, file))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let all_equal = loop {
                let collected = files
                    .iter_mut()
                    .map(|(path, file)| {
                        Ok(file
                            .next()
                            .transpose()
                            .map_err(ResolveError::from_io(*path))?)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !collected.iter().all(|byte| byte == &collected[0]) {
                    break false;
                }
                if collected.iter().all(Option::is_none) {
                    break true;
                }
            };
            if all_equal {
                debug!("Multiple mods use the binary at {:?}, but these files are all equal", path);
            } else {
                // If we got here, there's a conflict! Ask user to resolve it.
                let choice = ask_for_resolve(
                    sink,
                    format!(
                        "Multiple mods are using the binary file {}. Please choose one you wish to use the file from",
                        path.to_string_lossy()
                    ),
                    changes,
                );
                out.insert(path, choice);
            }
        }
    }

    Ok(out)
}

pub fn resolve_added_text(
    sink: &mut cursive::CbSink,
    data: HashMap<String, &mut ModAddedTexts>,
) -> ModAddedTexts {
    let mut out = ModAddedTexts::new();
    for (path, changes) in regroup(data) {
        debug_assert!(!changes.is_empty());
        if changes.len() == 1 {
            out.insert(path, changes.into_iter().next().unwrap().1);
        } else {
            // Conflict!
            let choice = ask_for_resolve(
                sink,
                format!(
                    "Multiple mods are attempting to create structured file {}.
                    In this case, we create it based on the data from one mod and then
                    let other mods patch it.
                    
                    If one of the mods is providing some functionality and others serve
                    as \"patches\" or \"extensions\" to that functionality,
                    choose this \"core\" mod now.
                    Otherwise, choose the one which is the least important, as its data is
                    likely to be overwritten in case of conflict.",
                    path.to_string_lossy()
                ),
                changes,
            );
            out.insert(path, choice);
        }
    }
    out
}

pub fn resolve_modified_text(
    sink: &mut cursive::CbSink,
    original: &GameData,
    data: HashMap<String, &mut ModModifiedTexts>,
) -> ModModifiedTexts {
    let mut out = ModModifiedTexts::new();
    for (path, changes) in regroup(data) {
        debug_assert!(!changes.is_empty());
        // As always, single change is immediately OK.
        if changes.len() == 1 {
            out.insert(path, changes.into_iter().next().unwrap().1);
        } else {
            // OK, now we have to ask the format itself... what will it do with multiple changes?
            let base = original
                .get(&path)
                .expect("Attempt to change non-existing file");
            let merged = base.merge_patches(sink, changes);
            // ...and now, finally, insert them.
            out.insert(path, merged);
        }
    }
    out
}

fn regroup<T>(
    input: HashMap<String, &mut HashMap<PathBuf, T>>,
) -> HashMap<PathBuf, Vec<(String, T)>> {
    let mut changes: HashMap<PathBuf, Vec<(String, T)>> = HashMap::new();
    for (mod_name, mod_changes) in input {
        for (path, item) in mod_changes.drain() {
            // False positive from clippy - https://github.com/rust-lang/rust-clippy/issues/5693
            #[allow(clippy::or_fun_call)]
            changes
                .entry(path)
                .or_insert(vec![])
                .push((mod_name.clone(), item));
        }
    }
    changes
}

fn ask_for_resolve<T: Debug + Send + Clone + 'static>(
    sink: &mut cursive::CbSink,
    text: impl Into<String>,
    options: impl IntoIterator<Item = (String, T)>,
) -> T {
    let (sender, receiver) = bounded(0);
    let text = text.into();
    let options: Vec<_> = options.into_iter().collect();
    debug!(
        "[resolve]: Asking for source to be used, variants: {:?}",
        options.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    crate::run_update(sink, move |cursive| {
        crate::push_screen(
            cursive,
            Dialog::around(
                LinearLayout::vertical()
                    .child(TextView::new(text))
                    .child(Panel::new(SelectView::new().with_all(options).on_submit(
                        move |cursive, value| {
                            cursive.pop_layer();
                            let _ = sender.send(value.clone());
                        },
                    ))),
            ),
        );
    });
    receiver
        .recv()
        .expect("Sender was dropped without sending anything")
}
