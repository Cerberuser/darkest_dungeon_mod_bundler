use cursive::{
    traits::Finder,
    views::{Dialog, TextView},
};
use difference::{Changeset, Difference};
use log::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
};

pub type DataTree = BTreeMap<PathBuf, DataNode>;

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

pub type DiffTree = BTreeMap<PathBuf, DiffNode>;
// FIXME: this makes it possible for multiple mods with the same name to collide!
pub type Conflict = Vec<(String, DiffNode)>;
pub type Conflicts = HashMap<PathBuf, Conflict>;

#[derive(Clone, Debug)]
pub struct LinesChangeset(pub Vec<Option<LineChange>>);
impl LinesChangeset {
    fn diff(first: &str, second: &str) -> Self {
        let lines_count = first.split('\n').count();
        info!("Diff: {} lines in original file", lines_count);
        let mut inner = Vec::with_capacity(lines_count);
        let mut removed = vec![];
        let mut modification: Option<LineModification> = None;
        for diff in Changeset::new(first, second, "\n").diffs {
            match diff {
                Difference::Same(lines) => {
                    let unchanged = lines.split('\n').map(|_| None).collect::<Vec<_>>();
                    debug!(
                        "Found unchanged block of lines, length = {}",
                        unchanged.len()
                    );
                    if let Some(modification) = modification.take() {
                        debug!(
                            "Pushed pending modification: {}, lines: {}",
                            modification.kind(),
                            modification.lines()
                        );
                        inner.push(Some(LineChange::Modified(modification)));
                    }
                    debug!("Pushed pending removals, length = {}", removed.len());
                    inner.extend(removed.drain(..));
                    inner.extend(unchanged);
                }
                Difference::Add(lines) => {
                    let added: Vec<String> = lines.split('\n').map(String::from).collect();
                    debug!("Got added block of lines, length = {}", added.len());
                    for line in added {
                        match removed.pop() {
                            Some(_) => {
                                debug!("Found replacement for the previously deleted line");
                                if let Some(modification) = modification.take() {
                                    debug!(
                                        "Pushed pending modification: {}, lines: {}",
                                        modification.kind(),
                                        modification.lines()
                                    );
                                    inner.push(Some(LineChange::Modified(modification)));
                                }
                                modification = Some(LineModification::Replaced(line));
                            }
                            None => {
                                modification = match modification.take() {
                                    Some(LineModification::Replaced(s)) => {
                                        debug!("Line is added to the existing replacement block");
                                        Some(LineModification::Replaced(s + "\n" + &line))
                                    }
                                    Some(LineModification::Added(s)) => {
                                        debug!("Line is added to the existing addition block");
                                        Some(LineModification::Added(s + "\n" + &line))
                                    }
                                    None => {
                                        // Either the list is empty, or the last line was unchanged
                                        // (otherwise we'd get into another branch).
                                        match inner.pop() {
                                            Some(Some(_)) => debug_assert!(false, "Logic error in diff: new addition is generated with last line already changed"),
                                            Some(None) => debug!("Attaching addition to the already changed line"),
                                            // FIXME: this moves the change one line below
                                            None => debug!("Adding line after the first line of the file"),
                                        }
                                        debug!("Attaching the addition to the previous line");
                                        Some(LineModification::Added(line))
                                    }
                                }
                            }
                        }
                    }
                }
                Difference::Rem(lines) => {
                    let pending_removed = lines
                        .split('\n')
                        .map(|_| Some(LineChange::Removed))
                        .collect::<Vec<_>>();
                    debug!("Got a removed block, length = {}", pending_removed.len());
                    if let Some(modification) = modification.take() {
                        debug!(
                            "Pushed pending modification: {}, lines: {}",
                            modification.kind(),
                            modification.lines()
                        );
                        inner.push(Some(LineChange::Modified(modification)));
                    }
                    debug!("Pushed pending removals, length = {}", removed.len());
                    inner.extend(removed);
                    removed = pending_removed;
                }
            }
        }
        if let Some(modification) = modification {
            debug!(
                "Pushed pending modification: {}, lines: {}",
                modification.kind(),
                modification.lines()
            );
            inner.push(Some(LineChange::Modified(modification)));
        }
        debug!("Pushed pending removals, length = {}", removed.len());
        inner.extend(removed);
        debug_assert!(inner.len() == lines_count);
        info!("Calculated patches for every line");
        Self(inner)
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum LineModification {
    Replaced(String),
    Added(String),
}
impl LineModification {
    pub fn kind(&self) -> &str {
        match self {
            LineModification::Replaced(_) => "Replaced",
            LineModification::Added(_) => "Added",
        }
    }
    pub fn lines(&self) -> usize {
        match self {
            LineModification::Replaced(line) => line,
            LineModification::Added(line) => line,
        }
        .lines()
        .count()
    }
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
#[derive(Debug)]
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
    fn diff(&self, other: DataTree) -> DiffTree;
}
impl DataTreeExt for DataTree {
    fn diff(&self, other: DataTree) -> DiffTree {
        use DataNodeContent::*;
        other.into_iter().map(|(path, modded)| {
            info!("Comparing data on path {:?}", path);
            let value = match self.get(&path) {
                Some(orig) => {
                    info!("Mod is overwriting existing file {:?}", path);
                    match (&orig.content, &modded.content) {
                        (Binary, Binary) => {
                            info!("{:?} is a binary file - skipping diff", path);
                            DiffNode::Binary(modded.absolute)
                        }
                        (Text(orig), Text(modded)) => {
                            info!("{:?} is a text file - calculating diff", path);
                            DiffNode::ModifiedText(LinesChangeset::diff(orig, modded))
                        }
                        _ => {
                            panic!(
                                "Unexpected mismatch: original file {:?} and modded file {:?} have different kinds",
                                orig.absolute,
                                modded.absolute
                            )
                        },
                    }
                }
                None => {
                    info!("Mod is introducing new file {:?}", path);
                    match modded.content {
                        Binary => DiffNode::Binary(modded.absolute),
                        Text(modded) => DiffNode::AddedText(modded),
                    }
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
        Ok(merge(try_prepare_merge(self)?, on_progress))
    }
}
impl<I, E> ResultDiffTressExt<E> for I where I: Iterator<Item = Result<ModContent, E>> + Sized {}
pub trait DiffTreesExt: Iterator<Item = ModContent> + Sized {
    fn merge(self, on_progress: Option<&mut cursive::CbSink>) -> (DiffTree, Conflicts) {
        merge(prepare_merge(self), on_progress)
    }
}
impl<I> DiffTreesExt for I where I: Iterator<Item = ModContent> + Sized {}

type UsagesMap = HashMap<PathBuf, Vec<Rc<RefCell<ModContent>>>>;

fn add_usage(usages: &mut UsagesMap, diff: ModContent) {
    info!("Filling the list of files touched by mod: {}", diff.name);
    let diff = Rc::new(RefCell::new(diff));
    let borrowed = diff.borrow();
    for path in borrowed.diff.keys() {
        debug!("Mod {}, file {:?}", borrowed.name, path);
        // False positive from clippy - https://github.com/rust-lang/rust-clippy/issues/5693
        #[allow(clippy::or_fun_call)]
        usages
            .entry(path.clone())
            .or_insert(vec![])
            .push(Rc::clone(&diff));
    }
}

fn try_prepare_merge<E>(
    mods: impl IntoIterator<Item = Result<ModContent, E>>,
) -> Result<UsagesMap, E> {
    let mut usages = HashMap::new();
    for diff in mods {
        add_usage(&mut usages, diff?);
    }
    Ok(usages)
}

fn prepare_merge(mods: impl IntoIterator<Item = ModContent>) -> UsagesMap {
    let mut usages = HashMap::new();
    for diff in mods {
        add_usage(&mut usages, diff);
    }
    usages
}

fn merge(
    usages: UsagesMap,
    mut on_progress: Option<&mut cursive::CbSink>,
) -> (DiffTree, Conflicts) {
    let mut conflicts = Conflicts::new();
    let mut merged = DiffTree::new();

    if let Some(sink) = on_progress.as_mut() {
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
    }

    // Now, we'll iterate over files.
    for (path, mut mods) in usages {
        let string_path = path.to_string_lossy();
        info!("[merge] {:?}: merging changes", path);
        if let Some(sink) = on_progress.as_mut() {
            super::set_file_updated(sink, "Merging", string_path)
        }

        // Sanity check: mods vec shouldn't be empty.
        if mods.is_empty() {
            warn!(
                "[merge] {:?}: unexpected empty list of modifying mods",
                path
            );
            continue;
        }
        // The simplest case: file is modified by exactly one mod.
        else if mods.len() == 1 {
            // We can remove entry from DiffTree, since it won't be ever touched later.
            let the_mod = mods.remove(0);
            info!(
                "[merge] {:?}: no conflicts - file is changed only by mod {}",
                path,
                the_mod.borrow().name
            );
            let item = the_mod.borrow_mut().diff.remove(&path).unwrap();
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
                .collect::<Vec<_>>();
            info!(
                "[merge] {:?}: multiple mods are changing file: {:?}",
                path,
                list.iter().map(|(name, _)| name).collect::<Vec<_>>()
            );
            match kind {
                // Another simple case is when multiple mods modify (or create) one binary file.
                // For multiple mods adding the same text file, we want to ask user to choose one of them as "base",
                // and then we'll run the diffing again, with "base" being the "vanilla" and all others being "mods".
                // So, they are directly put into "conflicts", like the binaries.
                kind @ DiffNodeKind::Binary | kind @ DiffNodeKind::AddedText => {
                    debug!(
                        "[merge] {:?}: Diff is of kind {:?} - putting it to conflicts directly",
                        path, kind
                    );
                    conflicts.insert(path, list);
                }
                // Now that's getting tricky.
                DiffNodeKind::ModifiedText => {
                    debug!("[merge] {:?}: Diff is modifying existing text - trying to merge line-by-line", path);
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
                                    debug!(
                                        "[merge] {:?}: Mod {} changes line {}",
                                        path, name, index
                                    );
                                    line_changes[index].insert(name.into(), change.clone())
                                });
                            }
                        } else {
                            unreachable!();
                        }
                    }
                    // OK, now we get the list of every change grouped by source line.
                    let mut merged_changes = vec![];
                    for (index, line_change) in line_changes.into_iter().enumerate() {
                        // Trivial case - no changes
                        if line_change.is_empty() {
                            merged_changes.push(None);
                            for change in conflict_changes.values_mut() {
                                change.push(None);
                            }
                        }
                        // Good case - change from exactly one mod.
                        else if line_change.len() == 1 {
                            let (name, change) = line_change.into_iter().next().unwrap();
                            debug!(
                                "[merge] {:?}: Exactly one change for line {}, mod = {}",
                                path, index, name
                            );
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
                                debug!(
                                    "[merge] {:?}: Multiple equal changes for line {}",
                                    path, index
                                );
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
                            debug!(
                                "[merge] {:?}: Conflicting changes for line {}, mods: {:?}",
                                path,
                                index,
                                line_change.keys().collect::<Vec<_>>()
                            );
                            let mut line_change = line_change;
                            for (name, conflict) in conflict_changes.iter_mut() {
                                let change = line_change.remove(name);
                                conflict.push(change);
                            }
                        }
                    }
                    // Woof! Finally, we can put the results into the output maps.
                    if !merged_changes.iter().all(Option::is_none) {
                        info!("[merge] {:?}: outputting merged changes", path);
                        merged.insert(
                            path.clone(),
                            DiffNode::ModifiedText(LinesChangeset(merged_changes)),
                        );
                    }
                    conflict_changes.retain(|_, list| !list.iter().all(Option::is_none));
                    if !conflict_changes.is_empty() {
                        info!("[merge] {:?}: outputting conflicts", path);
                        let conflict_changes = conflict_changes
                            .into_iter()
                            .map(|(key, list)| {
                                debug!("[merge] {:?}: conflicting changes from mod {}", path, key);
                                (key, DiffNode::ModifiedText(LinesChangeset(list)))
                            })
                            .collect();
                        conflicts.insert(path, conflict_changes);
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
        info!("Applying calculated diff to the source tree");
        self.into_iter()
            .map(|(path, changes)| match changes {
                DiffNode::Binary(source) => {
                    debug!("[apply] {:?}: added binary file from {:?}", path, source);
                    (path, DataNode::new(source, None))
                },
                DiffNode::AddedText(text) => {
                    debug!("[apply] {:?}: added text", path);
                    (path, DataNode::new("", text))
                },
                DiffNode::ModifiedText(changeset) => {
                    debug!("[apply] {:?}: modified text", path);
                    let orig = match &original.get(&path).unwrap().content {
                        DataNodeContent::Binary => unreachable!(),
                        DataNodeContent::Text(text) => text,
                    };
                    let text = orig
                        .lines()
                        .zip(changeset.0)
                        .enumerate()
                        .filter_map(|(index, (orig, change))| match change {
                            Some(change) => match change {
                                LineChange::Removed => {
                                    debug!("[apply] {:?}: Removing line {}", path, index);
                                    None
                                }
                                LineChange::Modified(change) => match change {
                                    LineModification::Replaced(text) => {
                                        debug!("[apply] {:?}: Replacing line {} with {} new lines", path, index, text.lines().count());
                                        Some(text)
                                    },
                                    LineModification::Added(text) => {
                                        debug!("[apply] {:?}: Adding {} new lines after line {}", path, text.lines().count(), index);
                                        Some(format!("{}\n{}", orig, text))
                                    }
                                },
                            },
                            None => Some(orig.into()),
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    (path, DataNode::new("", text))
                }
            })
            .collect()
    }
}
