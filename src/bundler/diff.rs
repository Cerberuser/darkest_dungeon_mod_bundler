use difference::{Changeset, Difference};
use log::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
};
use cursive::{traits::Finder, views::{TextView, Dialog}};

pub type DataTree = HashMap<PathBuf, DataNode>;

pub struct DataNode {
    absolute: PathBuf,
    content: DataNodeContent,
}

impl DataNode {
    pub fn new(path: impl Into<PathBuf>, content: impl Into<DataNodeContent>) -> Self {
        Self {
            absolute: path.into(),
            content: content.into(),
        }
    }
    pub fn into_content(self) -> DataNodeContent {
        self.content
    }
}

#[derive(Debug)]
pub enum DataNodeContent {
    Binary,
    Text(String),
}

impl From<String> for DataNodeContent {
    fn from(content: String) -> Self {
        Self::Text(content)
    }
}
impl From<Option<String>> for DataNodeContent {
    fn from(content: Option<String>) -> Self {
        match content {
            Some(content) => Self::Text(content),
            None => Self::Binary,
        }
    }
}

pub struct ModContent {
    name: String,
    diff: DiffTree,
}
impl ModContent {
    pub fn new(name: impl Into<String>, diff: DiffTree) -> Self {
        Self {
            name: name.into(),
            diff,
        }
    }
}

pub type DiffTree = HashMap<PathBuf, DiffNode>;
// FIXME: this makes it possible for multiple mods with the same name to collide!
pub type Conflict = Vec<(String, DiffNode)>;
pub type Conflicts = HashMap<PathBuf, Conflict>;

#[derive(Clone)]
pub struct LinesChangeset(Vec<LineChange>);
#[derive(PartialEq, Eq, Clone)]
pub enum LineChange {
    Same,
    Removed,
    Replaced(String),
    Added(String),
}
impl LinesChangeset {
    fn diff(first: &str, second: &str) -> Self {
        let mut inner = vec![];
        let mut removed = vec![];
        for diff in Changeset::new(first, second, "\n").diffs {
            match diff {
                Difference::Same(lines) => {
                    if !removed.is_empty() {
                        inner.extend(removed.drain(..));
                    }
                    inner.extend(lines.split("\n").map(|_| LineChange::Same))
                }
                Difference::Add(lines) => {
                    let lines: Vec<String> = lines.split("\n").map(String::from).collect();
                    for line in lines {
                        match removed.pop() {
                            Some(_) => inner.push(LineChange::Replaced(line)),
                            None => inner.push(LineChange::Added(line)),
                        }
                    }
                }
                Difference::Rem(lines) => {
                    debug_assert!(removed.is_empty());
                    removed = lines.split('\n').map(|_| LineChange::Removed).collect();
                }
            }
        }
        inner.extend(removed);
        Self(inner)
    }
}

pub struct ModLineModification {
    replacement: Option<String>,
    added: Vec<String>,
}
pub enum ModLineChange {
    Removed,
    Modified(ModLineModification),
}

pub enum DiffNode {
    Binary(PathBuf),
    AddedText(String),
    ModifiedText(LinesChangeset),
}
pub enum DiffNodeKind {
    Binary,
    AddedText,
    ModifiedText,
}
impl DiffNode {
    pub fn kind(&self) -> DiffNodeKind {
        match self {
            DiffNode::Binary(_) => DiffNodeKind::Binary,
            DiffNode::AddedText(_) => DiffNodeKind::AddedText,
            DiffNode::ModifiedText(_) => DiffNodeKind::ModifiedText,
        }
    }
}

pub trait DataTreeExt {
    fn merge(&mut self, other: DataTree);
    fn diff(&self, other: DataTree) -> DiffTree;
}
impl DataTreeExt for DataTree {
    fn merge(&mut self, other: DataTree) {
        self.extend(other)
    }
    fn diff(&self, other: DataTree) -> DiffTree {
        use DataNodeContent::*;
        other.into_iter().map(|(path, modded)| {
            let value = match self.get(&path) {
                Some(orig) => match (&orig.content, &modded.content) {
                    (Binary, Binary) => DiffNode::Binary(modded.absolute),
                    (Text(orig), Text(modded)) => DiffNode::ModifiedText(LinesChangeset::diff(orig, modded)),
                    _ => panic!(
                        "Unexpected mismatch: original file {:?} and modded file {:?} have different kinds",
                        orig.absolute,
                        modded.absolute
                    ),
                }
                None => match modded.content {
                    Binary => DiffNode::Binary(modded.absolute),
                    Text(modded) => DiffNode::AddedText(modded),
                }
            };
            (path, value)
        }).collect()
    }
}

pub trait ResultDiffTressExt<E>: Iterator<Item = Result<ModContent, E>> + Sized {
    fn try_merge(self, on_progress: Option<&mut cursive::CbSink>) -> Result<(DiffTree, Conflicts), E> {
        let res = self.collect::<Result<Vec<_>, _>>()?;
        Ok(merge(res, on_progress))
    }
}
impl<I, E> ResultDiffTressExt<E> for I where I: Iterator<Item = Result<ModContent, E>> + Sized {}
pub trait DiffTreesExt: Iterator<Item = ModContent> + Sized {
    fn merge(self, on_progress: Option<&mut cursive::CbSink>) -> (DiffTree, Conflicts) {
        merge(self, on_progress)
    }
}
impl<I> DiffTreesExt for I where I: Iterator<Item = ModContent> + Sized {}

pub fn merge(
    diffs: impl IntoIterator<Item = ModContent>,
    mut on_progress: Option<&mut cursive::CbSink>,
) -> (DiffTree, Conflicts) {
    let mut conflicts = Conflicts::new();
    let mut merged = DiffTree::new();

    on_progress.as_mut().map(|sink| crate::run_update(sink, |cursive| {
        cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
            dialog.set_title("Merging fetched mods...");
            dialog.call_on_name("Loading part", |text: &mut TextView| {
                text.set_content(" ");
            });
            dialog.call_on_name("Loading file", |text: &mut TextView| {
                text.set_content(" ");
            });
        });
    }));

    // First, we'll fill the map which shows every mod touching some file.
    let mut usages: HashMap<PathBuf, Vec<Rc<RefCell<ModContent>>>> = HashMap::new();
    for diff in diffs {
        let diff = Rc::new(RefCell::new(diff));
        for (path, _) in &diff.borrow().diff {
            usages
                .entry(path.clone())
                .or_insert(vec![])
                .push(Rc::clone(&diff));
        }
    }

    // Now, we'll operate on files.
    for (path, mut mods) in usages {
        let string_path = path.to_string_lossy().to_string();
        on_progress.as_mut().map(|sink| super::set_file_updated(sink, "Merging".into(), string_path));

        // Sanity check: mods vec shouldn't be empty.
        if mods.is_empty() {
            warn!(
                "Unexpected empty list of modifying mods for file {:?}",
                path
            );
            continue;
        }
        // The simplest case: file is modified by exactly one mod.
        else if mods.len() == 1 {
            // We can remove entry from DiffTree, since it won't be ever touched later.
            let item = mods.remove(0).borrow_mut().diff.remove(&path).unwrap();
            merged.insert(path, item);
        }
        // Now, we should check what kind of changes are there.
        else {
            let kind = mods[0].borrow().diff.get(&path).unwrap().kind();
            let list = mods
                .into_iter()
                .map(|item| {
                    let mut item = item.borrow_mut();
                    (item.name.clone(), item.diff.remove(&path).unwrap())
                })
                .collect();
            match kind {
                // Another simple case is when multiple mods modify (or create) one binary file.
                // For multiple mods adding the same text file, we want to ask user to choose one of them as "base",
                // and then we'll run the diffing again, with "base" being the "vanilla" and all others being "mods".
                // So, they are directly put into "conflicts", like the binaries.
                DiffNodeKind::Binary | DiffNodeKind::AddedText => {
                    conflicts.insert(path, list);
                }
                // Now that's getting tricky.
                DiffNodeKind::ModifiedText => {
                    // We will treat as conflict any case when two mods modify the same line.
                    // And we want to merge all non-conflicting cases.
                    // So, we iterate over every changeset, to check which lines are
                    // changed by it.
                    let mut line_changes: Vec<HashMap<String, ModLineChange>> = vec![];
                    let mut conflict_changes = HashMap::new();
                    for changes in &list {
                        if let (name, DiffNode::ModifiedText(changelist)) = changes {
                            conflict_changes.insert(name.to_string(), vec![]);
                            let mut num = 0;
                            let mut pending_change: Option<ModLineModification> = None;
                            if line_changes.is_empty() {
                                line_changes.resize_with(changelist.0.len(), Default::default);
                            }
                            for change in &changelist.0 {
                                // Then, if we're not adding a new line, we have to flush any pending change.
                                if let LineChange::Added(_) = change {
                                } else {
                                    if let Some(pending_change) = pending_change.take() {
                                        line_changes[num].insert(
                                            name.into(),
                                            ModLineChange::Modified(pending_change),
                                        );
                                        num += 1;
                                    }
                                }
                                match change {
                                    // If the line is unchanged, all we have to do is flush the pending change, if there is any.
                                    LineChange::Same => {
                                        // Nothing more to do - carrying on.
                                        num += 1;
                                    }
                                    // If the line is removed, we can store this fact immediately.
                                    LineChange::Removed => {
                                        line_changes[num]
                                            .insert(name.into(), ModLineChange::Removed);
                                        num += 1;
                                    }
                                    // If the line is replaced, we have to store this fact for future flushing,
                                    // since there may be follow-up additions,
                                    // and not put it immediately.
                                    LineChange::Replaced(repl) => {
                                        // We know that there's no pending change already - it was flushed before match.
                                        pending_change = Some(ModLineModification {
                                            replacement: Some(repl.into()),
                                            added: vec![],
                                        });
                                    }
                                    // If the line is added, we also defer this change,
                                    // either by creating a new pending change (without replacement) or by pushing into the existing one.
                                    LineChange::Added(line) => {
                                        pending_change
                                            .get_or_insert(ModLineModification {
                                                replacement: None,
                                                added: vec![],
                                            })
                                            .added
                                            .push(line.into());
                                    }
                                }
                            }
                            // The loop has ended - flush the pending change at the end, if any.
                            if let Some(pending_change) = pending_change.take() {
                                line_changes[num]
                                    .insert(name.into(), ModLineChange::Modified(pending_change));
                            }
                        } else {
                            unreachable!();
                        }
                    }
                    // OK, now we get the list of every change grouped by source line.
                    let mut merged_changes: Vec<LineChange> = vec![];
                    for line_change in line_changes {
                        // Trivial case - no changes
                        if line_change.len() == 0 {
                            merged_changes.push(LineChange::Same);
                            for change in conflict_changes.values_mut() {
                                change.push(LineChange::Same);
                            }
                        }
                        // Good case - change from exactly one mod.
                        else if line_change.len() == 1 {
                            let (_, change) = line_change.into_iter().next().unwrap();
                            match change {
                                ModLineChange::Removed => merged_changes.push(LineChange::Removed),
                                // A little tricky part - to get back per-line changes.
                                ModLineChange::Modified(modification) => {
                                    if let Some(repl) = modification.replacement {
                                        merged_changes.push(LineChange::Replaced(repl));
                                    }
                                    merged_changes.extend(modification.added.into_iter().map(LineChange::Added));
                                }
                            }
                            for change in conflict_changes.values_mut() {
                                change.push(LineChange::Same);
                            }
                        }
                        // Bad case - there's a conflict!
                        else {
                            // First of all, push "unchanged" marker to the merges list.
                            merged_changes.push(LineChange::Same);
                            // Now, let's operate on "conflicts".
                            let mut line_change = line_change;
                            for (name, conflict) in conflict_changes.iter_mut() {
                                let change = line_change.remove(name).map(|change| match change {
                                    ModLineChange::Removed => vec![LineChange::Removed],
                                    ModLineChange::Modified(modification) => {
                                        modification.replacement.map(LineChange::Replaced)
                                            .into_iter()
                                            .chain(modification.added.into_iter().map(LineChange::Added))
                                            .collect()
                                    }
                                }).unwrap_or(vec![LineChange::Same]);
                                conflict.extend(change);
                            }
                        }
                    }
                    // Woof! Finally, we can put the results into the output maps.
                    if !merged_changes.iter().all(|c| c == &LineChange::Same) {
                        merged.insert(
                            path.clone(),
                            DiffNode::ModifiedText(LinesChangeset(merged_changes)),
                        );
                    }
                    conflict_changes.retain(|_, list| !list.iter().all(|c| c == &LineChange::Same));
                    if conflict_changes.len() > 0 {
                        conflicts.insert(
                            path,
                            conflict_changes
                                .into_iter()
                                .map(|(key, list)| {
                                    (key, DiffNode::ModifiedText(LinesChangeset(list)))
                                })
                                .collect(),
                        );
                    }
                }
            }
        }
    }

    (merged, conflicts)
}

pub trait DiffTreeExt: Sized {
    fn apply_to(self, original: DataTree) -> DataTree;
}
impl DiffTreeExt for DiffTree {
    fn apply_to(self, original: DataTree) -> DataTree {
        self.into_iter().map(|(path, changes)| {
            match changes {
                DiffNode::Binary(source) => (path, DataNode::new(source, None)),
                DiffNode::AddedText(text) => (path, DataNode::new("", text)),
                DiffNode::ModifiedText(_) => {
                    
                    todo!();
                }
            }
        }).collect()
    }
}
