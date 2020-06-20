use super::diff::{
    Conflict, Conflicts, DataNode, DataNodeContent, DataTree, DataTreeExt, DiffNode, DiffNodeKind,
    DiffTree, DiffTreeExt, DiffTreesExt, LineChange, LineModification, LinesChangeset, ModContent,
};
use crossbeam_channel::bounded;
use cursive::{
    traits::{Nameable, Boxable},
    views::{Dialog, EditView},
};
use std::fmt::Debug;
use std::{collections::HashSet, path::PathBuf};

pub fn resolve(sink: &mut cursive::CbSink, conflicts: Conflicts) -> DiffTree {
    conflicts
        .into_iter()
        .map(|(path, conflict)| {
            let kind = conflict[0].1.kind();
            match kind {
                DiffNodeKind::AddedText => {
                    let (base, changes) = resolve_added_text(sink, path.clone(), conflict);
                    // Here, we have to do a little differently, since we're essentially resolving conflict
                    // by applying two actions, but have to make them as one.
                    let base: DataTree = vec![(path.clone(), DataNode::new(path.clone(), base))]
                        .into_iter()
                        .collect();
                    let changes: DiffTree = vec![(path.clone(), DiffNode::ModifiedText(changes))]
                        .into_iter()
                        .collect();
                    match changes.apply_to(base).remove(&path).unwrap().into_content() {
                        DataNodeContent::Text(text) => (path, DiffNode::AddedText(text)),
                        _ => unreachable!(),
                    }
                }
                DiffNodeKind::Binary => {
                    let resolved = resolve_binary(sink, path.clone(), conflict);
                    (path, DiffNode::Binary(resolved))
                }
                DiffNodeKind::ModifiedText => {
                    let resolved = resolve_modified_text(sink, path.clone(), conflict);
                    (path, DiffNode::ModifiedText(resolved))
                }
            }
        })
        .collect()
}

pub fn merge_resolved(merged: DiffTree, resolved: DiffTree) -> DiffTree {
    let (merged, conflicts) = vec![
        ModContent::new("merged", merged),
        ModContent::new("resolved", resolved),
    ]
    .into_iter()
    .merge(None);
    debug_assert!(conflicts.len() == 0);
    merged
}

fn ask_for_resolve<T: Debug + Send + Clone + 'static>(
    sink: &mut cursive::CbSink,
    text: impl Into<String>,
    options: impl IntoIterator<Item = (String, T)>,
) -> T {
    let (sender, receiver) = bounded(0);
    let text = text.into();
    let options: Vec<_> = options.into_iter().collect();
    crate::run_update(sink, move |cursive| {
        crate::push_screen(
            cursive,
            Dialog::around(
                cursive::views::LinearLayout::vertical()
                    .child(cursive::views::TextView::new(text))
                    .child(
                        cursive::views::SelectView::new()
                            .with_all(options)
                            .on_submit(move |cursive, value| {
                                cursive.pop_layer();
                                let _ = sender.send(value.clone());
                            }),
                    ),
            ),
        );
    });
    receiver
        .recv()
        .expect("Sender was dropped without sending anything")
}

fn resolve_binary(sink: &mut cursive::CbSink, target: PathBuf, conflict: Conflict) -> PathBuf {
    let variants = conflict.into_iter().map(|(name, node)| match node {
        DiffNode::Binary(path) => (name, path),
        _ => unreachable!(),
    });
    ask_for_resolve(
        sink,
        format!(
            "Multiple mods are using the binary file {}. Please choose one you wish to use",
            target.to_string_lossy()
        ),
        variants,
    )
}

fn render_line_choice(line: String) -> impl cursive::View {
    cursive::views::LinearLayout::horizontal()
        .child(cursive::views::TextView::new(line.clone()).full_width())
        .child(cursive::views::Button::new("Use this", move |cursive| {
            let line = line.clone();
            cursive.call_on_name("Line resolve edit", move |edit: &mut EditView| {
                edit.set_content(line)
            });
        }))
}

fn choose_line(
    sink: &mut cursive::CbSink,
    index: usize,
    file: impl Into<PathBuf>,
    lines: impl IntoIterator<Item = String>,
) -> Option<String> {
    let lines: Vec<_> = lines.into_iter().collect();
    let file = file.into();
    let (sender, receiver) = bounded(0);

    crate::run_update(sink, move |cursive| {
        let mut layout = cursive::views::LinearLayout::vertical();
        lines.into_iter().for_each(|line| layout.add_child(render_line_choice(line)));
        crate::push_screen(
            cursive,
            Dialog::around(
                layout.child(EditView::new().with_name("Line resolve edit").full_width())
            ).title(format!("Resolving line {} in file {}", index, file.to_string_lossy())).button("Resolve", move |cursive| {
                let value = cursive.call_on_name("Line resolve edit", |edit: &mut EditView| edit.get_content()).unwrap();
                cursive.pop_layer();
                let value = match &*value.as_str() {
                    "" => None,
                    val => Some(val.to_string()),
                };
                sender.send(value).unwrap();
            }),
        );
    });
    receiver
        .recv()
        .expect("Sender was dropped without sending anything")
}

fn resolve_changes_manually(
    sink: &mut cursive::CbSink,
    target: PathBuf,
    conflict: Conflict,
) -> LinesChangeset {
    let changes: Vec<_> = conflict
        .into_iter()
        .map(|(_, node)| match node {
            DiffNode::ModifiedText(changeset) => changeset.0,
            _ => unreachable!(),
        })
        .collect();
    // We do some kind of "transpose" for this vec, since we want to go from per-file to per-line interpretation.
    debug_assert!(changes.iter().map(Vec::len).collect::<HashSet<_>>().len() == 1);
    let mut line_changes = vec![vec![]; changes[0].len()];
    for change in changes {
        line_changes
            .iter_mut()
            .zip(change)
            .for_each(|(v, change)| v.push(change));
    }
    let line_changes: Vec<Option<Vec<_>>> = line_changes
        .into_iter()
        .map(|v| {
            debug_assert!(v.iter().all(Option::is_some) || v.iter().all(Option::is_none));
            v.into_iter().collect()
        })
        .collect();

    let changes = line_changes
        .into_iter()
        .enumerate()
        .map(|(index, change)| {
            change.map(|change| {
                let options = change.into_iter().map(|change| match change {
                    LineChange::Removed => "".into(),
                    LineChange::Modified(modification) => {
                        if modification.added.len() > 0 {
                            unimplemented!(); // FIXME - handle this condition gracefully
                        }
                        match modification.replacement {
                            Some(line) => line,
                            None => unimplemented!(), // FIXME - the same
                        }
                    }
                });
                match choose_line(sink, index, &target, options) {
                    Some(line) => LineChange::Modified(LineModification {
                        replacement: Some(line),
                        added: vec![],
                    }),
                    None => LineChange::Removed,
                }
            })
        })
        .collect();
    LinesChangeset(changes)
}

fn resolve_modified_text(
    sink: &mut cursive::CbSink,
    target: PathBuf,
    conflict: Conflict,
) -> LinesChangeset {
    // Clone conflict, to use it later in manual resolution if necessary
    let variants = conflict
        .clone()
        .into_iter()
        .map(|(name, node)| match node {
            DiffNode::ModifiedText(changeset) => (name, Some(changeset)),
            _ => unreachable!(),
        })
        .chain(std::iter::once(("Resolve manually".into(), None)));
    let changeset = ask_for_resolve(
        sink,
        format!(
            "Multiple mods are modifying the text file {}. Please choose one you wish to use",
            target.to_string_lossy()
        ),
        variants,
    );
    match changeset {
        Some(changeset) => changeset,
        None => resolve_changes_manually(sink, target, conflict),
    }
}

fn resolve_added_text(
    sink: &mut cursive::CbSink,
    target: PathBuf,
    conflict: Conflict,
) -> (String, LinesChangeset) {
    // First, store the data a little more appropriately.
    let mut data: std::collections::HashMap<_, _> = conflict
        .into_iter()
        .map(|(name, node)| match node {
            DiffNode::AddedText(text) => (name, text),
            _ => unreachable!(),
        })
        .collect();

    let variants = data.keys().cloned().map(|name| (name.clone(), name));
    let choice = ask_for_resolve(
        sink,
        format!(
            "Multiple mods are modifying the text file {}. Please choose one you wish to use",
            target.to_string_lossy()
        ),
        variants,
    );
    let chosen = data.remove(&choice).unwrap();
    let base: DataTree = vec![(target.clone(), DataNode::new("", chosen.clone()))]
        .into_iter()
        .collect();

    let (merged, conflicts) = data
        .into_iter()
        .map(|(name, content)| {
            ModContent::new(
                name.clone(),
                base.diff(
                    vec![(target.clone(), DataNode::new(name, content))]
                        .into_iter()
                        .collect(),
                ),
            )
        })
        .merge(None);
    let resolved = resolve(sink, conflicts);
    let mut merged = merge_resolved(merged, resolved);

    let changeset = match merged.remove(&target) {
        Some(changes) => match changes {
            DiffNode::ModifiedText(changeset) => changeset,
            _ => unreachable!(),
        },
        None => unreachable!(),
    };

    (chosen, changeset)
}