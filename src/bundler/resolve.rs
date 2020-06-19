use super::diff::{
    Conflict, Conflicts, DataNode, DataNodeContent, DataTree, DataTreeExt, DiffNode, DiffNodeKind,
    DiffTree, DiffTreeExt, DiffTreesExt, LinesChangeset, ModContent,
};
use crossbeam_channel::bounded;
use cursive::views::Dialog;
use std::path::PathBuf;

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

fn ask_for_resolve<T: Send + Clone + 'static>(
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

// TODO: this conflict can also be resolved manually, so here it'd be better to make separate handling
fn resolve_modified_text(
    sink: &mut cursive::CbSink,
    target: PathBuf,
    conflict: Conflict,
) -> LinesChangeset {
    let variants = conflict.into_iter().map(|(name, node)| match node {
        DiffNode::ModifiedText(changeset) => (name, changeset),
        _ => unreachable!(),
    });
    ask_for_resolve(
        sink,
        format!(
            "Multiple mods are modifying the text file {}. Please choose one you wish to use",
            target.to_string_lossy()
        ),
        variants,
    )
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
