use cursive::{
    traits::Finder,
    views::{Dialog, TextView},
};
use difference::{Changeset, Difference};
use log::*;
use std::{cell::RefCell, collections::{HashSet, HashMap}, path::PathBuf, rc::Rc};

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
    pub fn into_parts(self) -> (PathBuf, DataNodeContent) {
        (self.absolute, self.content)
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
        Self::Text(content.replace("\r\n", "\n"))
    }
}
impl From<Option<String>> for DataNodeContent {
    fn from(content: Option<String>) -> Self {
        match content {
            Some(content) => Self::Text(content.replace("\r\n", "\n")),
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

#[derive(Clone, Debug)]
pub struct LinesChangeset(pub Vec<Option<LineChange>>);
impl LinesChangeset {
    fn diff(first: &str, second: &str) -> Self {
        let lines_count = first.split("\n").count();
        let mut inner = Vec::with_capacity(lines_count);
        let mut removed = vec![];
        let mut modification = None;
        for diff in Changeset::new(first, second, "\n").diffs {
            match diff {
                Difference::Same(lines) => {
                    if let Some(modification) = modification.take() {
                        inner.push(Some(LineChange::Modified(modification)));
                    }
                    inner.extend(removed.drain(..));
                    inner.extend(lines.split("\n").map(|_| None));
                }
                Difference::Add(lines) => {
                    let added: Vec<String> = lines.split("\n").map(String::from).collect();
                    for line in added {
                        match removed.pop() {
                            Some(_) => {
                                if let Some(modification) = modification.take() {
                                    inner.push(Some(LineChange::Modified(modification)));
                                }
                                modification = Some(LineModification::Replaced(line));
                            }
                            None => {
                                modification = match modification.take() {
                                    Some(LineModification::Replaced(s)) => Some(LineModification::Replaced(s + "\n" + &line)),
                                    Some(LineModification::Added(s)) => Some(LineModification::Added(s + "\n" + &line)),
                                    None => {
                                        let last = inner.pop();
                                        // Either the list is empty, or the last line was unchanged
                                        // (otherwise we'd get into another branch).
                                        debug_assert!(last.is_none() || last.unwrap().is_none());
                                        Some(LineModification::Added(line))
                                    }
                                }
                            }
                        }
                    }
                }
                Difference::Rem(lines) => {
                    if let Some(modification) = modification.take() {
                        inner.push(Some(LineChange::Modified(modification)));
                    }
                    inner.extend(removed);
                    removed = lines
                        .split('\n')
                        .map(|_| Some(LineChange::Removed))
                        .collect();
                }
            }
        }
        if let Some(modification) = modification {
            inner.push(Some(LineChange::Modified(modification)));
        }
        inner.extend(removed);
        debug_assert!(inner.len() == lines_count);
        Self(inner)
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum LineModification {
    Replaced(String),
    Added(String),
}
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum LineChange {
    Removed,
    Modified(LineModification),
}

#[derive(Clone)]
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
                    _ => {
                        error!("Unexpected kinds mismatch");
                        panic!(
                        "Unexpected mismatch: original file {:?} and modded file {:?} have different kinds",
                        orig.absolute,
                        modded.absolute
                    )
                },
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
    fn try_merge(
        self,
        on_progress: Option<&mut cursive::CbSink>,
    ) -> Result<(DiffTree, Conflicts), E> {
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

    on_progress.as_mut().map(|sink| {
        crate::run_update(sink, |cursive| {
            cursive.call_on_name("Loading dialog", |dialog: &mut Dialog| {
                dialog.set_title("Merging fetched mods...");
                dialog.call_on_name("Loading part", |text: &mut TextView| {
                    text.set_content(" ");
                });
                dialog.call_on_name("Loading file", |text: &mut TextView| {
                    text.set_content(" ");
                });
            });
        })
    });

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
        on_progress
            .as_mut()
            .map(|sink| super::set_file_updated(sink, "Merging".into(), string_path));

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
                    let mut line_changes: Vec<HashMap<String, LineChange>> = vec![];
                    let mut conflict_changes = HashMap::new();
                    for changes in &list {
                        if let (name, DiffNode::ModifiedText(changelist)) = changes {
                            conflict_changes.insert(name.to_string(), vec![]);
                            if line_changes.is_empty() {
                                line_changes.resize_with(changelist.0.len(), Default::default);
                            }
                            for (index, change) in changelist.0.iter().enumerate() {
                                change.as_ref().map(|change| {
                                    line_changes[index].insert(name.into(), change.clone())
                                });
                            }
                        } else {
                            unreachable!();
                        }
                    }
                    // OK, now we get the list of every change grouped by source line.
                    let mut merged_changes = vec![];
                    for line_change in line_changes {
                        // Trivial case - no changes
                        if line_change.len() == 0 {
                            merged_changes.push(None);
                            for change in conflict_changes.values_mut() {
                                change.push(None);
                            }
                        }
                        // Good case - change from exactly one mod.
                        else if line_change.len() == 1 {
                            let (_, change) = line_change.into_iter().next().unwrap();
                            merged_changes.push(Some(change));
                            for change in conflict_changes.values_mut() {
                                change.push(None);
                            }
                        }
                        // Bad case - there's a conflict!
                        else {
                            // Don't panic yet! Let's check if all the changes are indeed the same.
                            let set: HashSet<_> = line_change.values().collect();
                            if set.len() == 1 {
                                // All changes are equal - no problem!
                                let (_, change) = line_change.into_iter().next().unwrap();
                                merged_changes.push(Some(change));
                                for change in conflict_changes.values_mut() {
                                    change.push(None);
                                }
                                continue;
                            }
                            // OK, that's really a conflict.
                            // First of all, push "unchanged" marker to the merges list.
                            merged_changes.push(None);
                            // Now, let's operate on "conflicts".
                            let mut line_change = line_change;
                            for (name, conflict) in conflict_changes.iter_mut() {
                                let change = line_change
                                    .remove(name);
                                conflict.push(change);
                            }
                        }
                    }
                    // Woof! Finally, we can put the results into the output maps.
                    if !merged_changes.iter().all(Option::is_none) {
                        merged.insert(
                            path.clone(),
                            DiffNode::ModifiedText(LinesChangeset(merged_changes)),
                        );
                    }
                    conflict_changes.retain(|_, list| !list.iter().all(Option::is_none));
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
    fn apply_to(self, _: DataTree) -> DataTree;
}
impl DiffTreeExt for DiffTree {
    fn apply_to(self, original: DataTree) -> DataTree {
        self.into_iter()
            .map(|(path, changes)| match changes {
                DiffNode::Binary(source) => (path, DataNode::new(source, None)),
                DiffNode::AddedText(text) => (path, DataNode::new("", text)),
                DiffNode::ModifiedText(changeset) => {
                    let orig = match &original.get(&path).unwrap().content {
                        DataNodeContent::Binary => unreachable!(),
                        DataNodeContent::Text(text) => text,
                    };
                    let text = orig.lines().zip(changeset.0).filter_map(|(orig, change)| match change {
                        Some(change) => match change {
                            LineChange::Removed => None,
                            LineChange::Modified(change) => match change {
                                LineModification::Replaced(text) => Some(text),
                                LineModification::Added(text) => Some(format!("{}\n{}", orig, text)),
                            }
                        },
                        None => Some(orig.into()),
                    }).collect::<Vec<_>>().join("\n");
                    (path, DataNode::new("", text))
                }
            })
            .collect()
    }
}
