use crate::bundler::{
    diff::{Conflicts, DataMap, ItemChange, Patch},
    game_data::{
        file_types::{darkest_parser, err_context, DarkestEntry},
        BTreeMapExt, BTreeMappable, BTreePatchable, BTreeSetable, DeployableStructured,
        GameDataValue, Loadable,
    },
    loader::utils::{collect_paths, ends_with},
    ModFileChange,
};
use combine::EasyParser;
use crossbeam_channel::{bounded, Sender};
use cursive::{
    traits::{Nameable, Resizable},
    views::{Button, Dialog, EditView, LinearLayout, Panel, ScrollView, TextArea, TextView},
};
use log::debug;
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryInto,
    fmt::Display,
    fs::File,
    io::{self, Write},
    num::ParseFloatError,
};

fn parse_percent(value: &str) -> Result<f32, ParseFloatError> {
    if value.ends_with('%') {
        Ok(value.trim_end_matches('%').parse::<f32>()? / 100.0)
    } else {
        value.parse()
    }
}

fn percent_to_string(value: f32) -> String {
    // Assume that the two decimal numbers are enough.
    format!("{:.2}%", value * 100.0)
}

#[derive(Clone, Debug)]
pub struct HeroInfo {
    id: String,
    resistances: Resistances,
    weapons: Weapons,
    armours: Armours,
    skills: Skills,
    riposte_skill: Option<Skill>,
    move_skill: Skill, // TODO - might be not the only one!
    tags: Vec<String>,
    extra_stack_limit: Vec<String>,
    deaths_door: DeathsDoor,
    modes: Modes,
    incompatible_party_member: Incompatibilities,
    unparsed: Unparsed,
    other: HashMap<(String, String), Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct HeroOverride {
    id: String,
    resistances: ResistancesOverride,
    weapons: Option<Weapons>,
    armours: Option<Armours>,
    skills: Option<Skills>,
    riposte_skill: Option<Skill>,
    move_skill: Option<Skill>,
    tags: Vec<String>,
    extra_stack_limit: Vec<String>,
    deaths_door: Option<DeathsDoor>,
    modes: Option<Modes>,
    incompatible_party_member: Option<Incompatibilities>,
    unparsed: Unparsed,
    other: HashMap<(String, String), Vec<String>>,
}

impl BTreeMappable for HeroInfo {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();

        out.extend_prefixed("resistances", self.resistances.to_map());
        out.extend_prefixed("weapons", self.weapons.to_map());
        out.extend_prefixed("armours", self.armours.to_map());
        out.extend_prefixed("skills", self.skills.to_map());
        if let Some(riposte_skill) = &self.riposte_skill {
            out.extend_prefixed("riposte_skill", riposte_skill.to_map());
        }
        out.extend_prefixed("move_skill", self.move_skill.to_map());
        out.extend_prefixed("tags", self.tags.to_set());
        out.extend_prefixed("extra_stack_limit", self.extra_stack_limit.to_set());
        out.extend_prefixed("deaths_door", self.deaths_door.to_map());
        out.extend_prefixed("modes", self.modes.to_map());
        out.extend_prefixed(
            "incompatible_party_member",
            self.incompatible_party_member.to_map(),
        );
        out.extend_prefixed("unparsed", self.unparsed.to_map());
        for (key, value) in &self.other {
            let mut intermid = DataMap::new();
            intermid.extend_prefixed(&key.1, value.to_set());
            let mut intermid_outer = DataMap::new();
            intermid_outer.extend_prefixed(&key.0, intermid);
            out.extend_prefixed("other", intermid_outer);
        }

        out
    }
}

impl BTreeMappable for HeroOverride {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();

        out.extend_prefixed("resistances", self.resistances.to_map());
        if let Some(weapons) = &self.weapons {
            out.extend_prefixed("weapons", weapons.to_map());
        }
        if let Some(armours) = &self.armours {
            out.extend_prefixed("armours", armours.to_map());
        }
        if let Some(skills) = &self.skills {
            out.extend_prefixed("skills", skills.to_map());
        }
        if let Some(riposte_skill) = &self.riposte_skill {
            out.extend_prefixed("riposte_skill", riposte_skill.to_map());
        }
        if let Some(move_skill) = &self.move_skill {
            out.extend_prefixed("move_skill", move_skill.to_map());
        }
        out.extend_prefixed("tags", self.tags.to_set());
        out.extend_prefixed("extra_stack_limit", self.extra_stack_limit.to_set());
        if let Some(deaths_door) = &self.deaths_door {
            out.extend_prefixed("deaths_door", deaths_door.to_map());
        }
        if let Some(modes) = &self.modes {
            out.extend_prefixed("modes", modes.to_map());
        }
        if let Some(incompatible_party_member) = &self.incompatible_party_member {
            out.extend_prefixed(
                "incompatible_party_member",
                incompatible_party_member.to_map(),
            );
        }
        out.extend_prefixed("unparsed", self.unparsed.to_map());
        for (key, value) in &self.other {
            let mut intermid = DataMap::new();
            intermid.extend_prefixed(&key.1, value.to_set());
            let mut intermid_outer = DataMap::new();
            intermid_outer.extend_prefixed(&key.0, intermid);
            out.extend_prefixed("other", intermid_outer);
        }

        out
    }
}

fn patch_list(list: &mut Vec<String>, mut path: Vec<String>, change: ItemChange) {
    // TODO - some debug assert to ensure that the path is correct
    let key = path.pop().unwrap();
    match change.into_option().map(GameDataValue::unwrap_string) {
        Some(_) => list.push(key),
        None => {
            // copied from Vec::remove_item
            let pos = list.iter().position(|x| x == &key).unwrap_or_else(|| {
                panic!(
                    "Unexpected key in hero info path: {:?}, attempt to remove non-existing entry",
                    path
                )
            });
            list.remove(pos);
        }
    };
}

impl BTreePatchable for HeroInfo {
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        for (mut path, change) in patch {
            match path.get(0).unwrap().as_str() {
                "resistances" => self.resistances.apply(path, change),
                "weapons" => self.weapons.apply(path, change),
                "armours" => self.armours.apply(path, change),
                "skills" => self.skills.apply(path, change),
                "riposte_skill" => self
                    .riposte_skill
                    .get_or_insert_with(Default::default)
                    .apply(path, change),
                "move_skill" => self.move_skill.apply(path, change),
                "tags" => patch_list(&mut self.tags, path, change),
                "extra_stack_limit" => patch_list(&mut self.extra_stack_limit, path, change),
                "deaths_door" => self.deaths_door.apply(path, change),
                "modes" => self.modes.apply(path, change),
                "incompatible_party_member" => self.incompatible_party_member.apply(path, change),
                "unparsed" => self.unparsed.apply(path, change),
                "other" => {
                    let first = path.remove(1);
                    let second = path.remove(2);
                    match change.into_option().map(GameDataValue::unwrap_string) {
                        Some(s) => {
                            self.other.entry((first, second)).or_default().push(s);
                        }
                        None => {
                            self.other.remove(&(first, second));
                        }
                    }
                }
                _ => panic!("Unexpected key in hero data patch: {:?}", path),
            }
        }
        Ok(())
    }
    fn try_merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Conflicts) {
        let mut merged = Patch::new();
        let mut unmerged = Conflicts::new();

        // TODO - this is almost the same as `regroup` in `resolve` module
        let mut changes = HashMap::new();
        for (mod_name, mod_changes) in patches {
            for (path, item) in mod_changes {
                // False positive from clippy - https://github.com/rust-lang/rust-clippy/issues/5693
                #[allow(clippy::or_fun_call)]
                changes
                    .entry(path)
                    .or_insert(vec![])
                    .push((mod_name.clone(), item));
            }
        }
        // Skills are treated separately, for the reason that we want to
        // resolve the changes to all the levels at once.
        // So, if there's any conflict on one level - other levels also go to "unmerged",
        // even if on them there is only one patched entry.
        for skill in self.skills.0.keys() {
            let skill_changes: Vec<_> = changes
                .iter()
                .filter(|(path, _)| path[0] == "skills" && &path[1] == skill)
                .map(|(path, changes)| (path.clone(), changes.clone()))
                .collect();
            let mut skill_map: HashMap<_, HashMap<_, _>> = HashMap::new();
            for (path, changes) in skill_changes {
                let level = path[2].clone();
                let path = path.iter().skip(3).cloned().collect::<Vec<_>>();
                skill_map.entry(path).or_default().insert(level, changes);
            }
            for (path, level_changes) in skill_map {
                if level_changes.values().all(|v| v.len() <= 1) {
                    // No conflicts for all levels - merging
                    for (level, level_change) in level_changes {
                        let full_path: Vec<_> = vec!["skills".into(), skill.clone(), level]
                            .into_iter()
                            .chain(path.clone())
                            .collect();
                        if let Some(change) = changes.remove(&full_path) {
                            debug_assert!(change.len() == 1);
                            debug_assert_eq!(level_change, change);
                            merged.insert(full_path, change.into_iter().next().unwrap().1);
                        }
                    }
                } else {
                    // Conflict on some level - dump all to unmerged
                    for (level, level_change) in level_changes {
                        let full_path: Vec<_> = vec!["skills".into(), skill.clone(), level]
                            .into_iter()
                            .chain(path.clone())
                            .collect();
                        let changes = changes.remove(&full_path).unwrap();
                        debug_assert_eq!(level_change, changes);
                        for change in changes {
                            unmerged.entry(full_path.clone()).or_default().push(change)
                        }
                    }
                }
            }
        }

        for (path, changes) in changes {
            debug_assert!(!changes.is_empty());
            if changes.len() == 1 {
                merged.insert(path, changes.into_iter().next().unwrap().1);
            } else {
                for change in changes {
                    unmerged.entry(path.clone()).or_default().push(change)
                }
            }
        }

        (merged, unmerged)
    }

    fn ask_for_resolve(&self, sink: &mut cursive::CbSink, mut conflicts: Conflicts) -> Patch {
        let mut out = Patch::new();

        // First, we want to separately merge everything connected to ordinary (non-riposte, non-move) skills.
        // The reason is simple - to let one merge all levels at once.
        let conflict_paths: Vec<_> = conflicts.keys().cloned().collect();
        for (skill, skill_data) in &self.skills.0 {
            let mut skill_changes: BTreeMap<_, BTreeMap<_, BTreeMap<_, _>>> = BTreeMap::new();
            for path in &conflict_paths {
                if path[0] == "skills" && &path[1] == skill {
                    // Drop the path from conflicts map, so that we don't have to filter it out later.
                    let change = conflicts
                        .remove(path)
                        .expect("The conflict value was used twice; this is a bug");
                    debug!("Got the change: path = {:?}, change = {:?}", path, change);
                    let level: i32 = path[2].parse().unwrap();
                    assert!(level >= 0 && level < 5);
                    let path = path.iter().skip(3).cloned().collect::<Vec<_>>();
                    for (mod_name, change) in change {
                        skill_changes
                            .entry(path.clone())
                            .or_default()
                            .entry(mod_name)
                            .or_default()
                            .insert(level, change);
                    }
                }
            }
            for (path, change) in skill_changes {
                // TODO: sanity check, for the case if all patches are in fact identical.

                // Here, we'll resolve all changes to the particular skill element.
                let entries = change
                    .into_iter()
                    .map(|(mod_name, change)| {
                        let change_text = change
                            .into_iter()
                            .map(|(level, change)| {
                                let mut value = change
                                    .into_option()
                                    .map(GameDataValue::unwrap_string)
                                    .unwrap_or_else(|| "<REMOVED>".into());
                                // Special case: effects are stored in map and patch at separate lines, but in UI they are on one line.
                                if path[0].as_str() == "effects" {
                                    value = value.replace('\n', " ");
                                }
                                (level, value)
                            })
                            .collect::<HashMap<_, _>>();
                        (mod_name, change_text)
                    })
                    .collect::<BTreeMap<_, _>>();
                let orig_text = skill_data
                    .iter()
                    .map(|(&level, skill)| {
                        let value = skill.get(&path);
                        (level, value)
                    })
                    .collect::<HashMap<_, _>>();

                let lines_output = |data: HashMap<i32, String>| {
                    let mut output = vec![];
                    for index in 0..5 {
                        let line = data
                            .get(&index)
                            .cloned()
                            .unwrap_or_else(|| "<UNCHANGED>".into());
                        output.push(format!("Level {}, value: {}", index, line));
                    }
                    output.join("\n")
                };
                let original_data: Vec<_> = (0..5)
                    .map(|index| skill_data.get(&index).map(|skill| skill.get(&path)))
                    .collect();
                let lines_input = move |data: HashMap<i32, String>| {
                    let mut output = vec![];
                    for index in 0..5 {
                        let line = data
                            .get(&index)
                            .cloned()
                            .or_else(|| original_data.clone().remove(index as usize))
                            .unwrap_or_default();
                        output.push(line);
                    }
                    output.join("\n")
                };
                let lines_input_output: Vec<_> =
                    std::iter::once(("Original values".into(), orig_text))
                        .chain(entries)
                        .map(|(name, lines)| {
                            (name, lines_input(lines.clone()), lines_output(lines))
                        })
                        .collect();

                let (sender, receiver) = bounded(0);
                let self_id = self.id.clone();
                let path_str = format!(
                    "skills / {} / <levels> / {}",
                    skill,
                    path.clone().join(" / ")
                );
                crate::run_update(sink, move |cursive| {
                    let mut layout = LinearLayout::vertical();
                    lines_input_output
                        .into_iter()
                        .for_each(|(name, lines_input, lines_output)| {
                            layout.add_child(
                                LinearLayout::horizontal()
                                    .child(
                                        Panel::new(TextView::new(lines_output).full_width())
                                            .title(name),
                                    )
                                    .child(Button::new("Move to input", move |cursive| {
                                        let lines_input = lines_input.clone();
                                        cursive.call_on_name(
                                            "Line resolve edit",
                                            |edit: &mut TextArea| edit.set_content(lines_input),
                                        );
                                    })),
                            )
                        });
                    let resolve_sender = sender.clone();
                    crate::push_screen(
                        cursive,
                        Dialog::around(
                            LinearLayout::vertical()
                                .child(ScrollView::new(layout))
                                .child(TextArea::new().with_name("Line resolve edit").full_width()),
                        )
                        .title(format!(
                            "Resolving entry: hero ID = {}, path = {}",
                            self_id, path_str
                        ))
                        .button("Resolve", move |cursive| {
                            let value = cursive
                                .call_on_name("Line resolve edit", |edit: &mut TextArea| {
                                    edit.get_content().split('\n').map(String::from).collect()
                                })
                                .unwrap();
                            cursive.pop_layer();
                            resolve_sender.send(Some(value)).unwrap();
                        })
                        .button("Remove", move |cursive| {
                            cursive.pop_layer();
                            sender.send(None).unwrap();
                        })
                        .h_align(cursive::align::HAlign::Center),
                    );
                });
                let choice: Option<Vec<String>> = receiver
                    .recv()
                    .expect("Sender was dropped without sending anything");
                match choice {
                    Some(items) => {
                        // Effects and non-effect entries are treated a little differently.
                        match path[0].as_str() {
                            "effects" => {
                                for (level, item) in items.into_iter().enumerate() {
                                    let full_path =
                                        vec!["skills".into(), skill.clone(), level.to_string()]
                                            .into_iter()
                                            .chain(path.clone())
                                            .collect();
                                    let (effects, rest) = DarkestEntry::values()
                                        .easy_parse(item.as_str())
                                        .expect("Wrong format for effects");
                                    assert!(
                                        rest.trim().is_empty(),
                                        "Something was left unparsed: {:?}",
                                        rest
                                    );
                                    out.insert(
                                        full_path,
                                        ItemChange::Set(effects.join("\n").into()),
                                    );
                                }
                            }
                            "other" => {
                                for (level, item) in items.into_iter().enumerate() {
                                    let full_path =
                                        vec!["skills".into(), skill.clone(), level.to_string()]
                                            .into_iter()
                                            .chain(path.clone())
                                            .collect();
                                    out.insert(full_path, ItemChange::Set(item.into()));
                                }
                            }
                            _ => panic!("Unexpected path in hero skills: {:?}", path),
                        }
                    }
                    None => {
                        // There might be less then five entries, in case of override.
                        for level in skill_data.keys() {
                            let full_path = vec!["skills".into(), skill.clone(), level.to_string()]
                                .into_iter()
                                .chain(path.clone())
                                .collect();
                            out.insert(full_path, ItemChange::Removed);
                        }
                    }
                }
            }
        }

        // Now, we can simply iterate over changes one-by-one.
        for (path, mut changes) in conflicts {
            // Sort the changes by mod name, just for convenience.
            changes.sort_by_key(|pair| pair.0.clone());
            // First, we can check if the resulting chains are really different (it's hard to do before).
            let mut iter = changes.iter();
            let (_, first_chain) = iter.next().unwrap();
            let all_equal = iter.all(|(_, chain)| chain == first_chain);
            let choice = if all_equal {
                // <HACK> (see below)
                first_chain
                    .clone()
                    .into_option()
                    .as_ref()
                    .map(GameDataValue::to_string)
            } else {
                let (sender, receiver) = bounded(0);
                let resolve_sender = Sender::clone(&sender);
                let self_id = self.id.clone();
                let path_str = path.join(" ");
                crate::run_update(sink, move |cursive| {
                    let mut layout = LinearLayout::vertical();
                    changes.into_iter().for_each(|(name, line)| {
                        let value = line.into_option();
                        layout.add_child(
                            LinearLayout::horizontal()
                                .child(
                                    Panel::new(
                                        TextView::new(
                                            value
                                                .as_ref()
                                                .map(GameDataValue::to_string)
                                                .unwrap_or_else(|| "<REMOVED>".into()),
                                        )
                                        .full_width(),
                                    )
                                    .title(name),
                                )
                                .child(Button::new("Move to input", move |cursive| {
                                    debug!("Moving value to the input line: {:?}", value);
                                    let res = cursive.call_on_name(
                                        "Line resolve edit",
                                        |edit: &mut EditView| {
                                            debug!("Setting EditView content: {:?}", value);
                                            edit.set_content(
                                                value
                                                    .as_ref()
                                                    .map(GameDataValue::to_string)
                                                    .unwrap_or_default(),
                                            )
                                        },
                                    );
                                    if res.is_none() {
                                        debug!("Failed to call Cursive callback");
                                        panic!();
                                    }
                                })),
                        )
                    });
                    crate::push_screen(
                        cursive,
                        Dialog::around(
                            layout
                                .child(EditView::new().with_name("Line resolve edit").full_width()),
                        )
                        .title(format!(
                            "Resolving hero info: hero ID = {}, path = {}",
                            self_id, path_str
                        ))
                        .button("Resolve", move |cursive| {
                            debug!("Sending \"set\" message");
                            let value = cursive
                                .call_on_name("Line resolve edit", |edit: &mut EditView| {
                                    edit.get_content().to_string()
                                })
                                .unwrap();
                            cursive.pop_layer();
                            resolve_sender.send(Some(value)).unwrap();
                        })
                        .button("Remove", move |cursive| {
                            cursive.pop_layer();
                            debug!("Sending \"remove\" message");
                            sender.send(None).unwrap();
                        })
                        .h_align(cursive::align::HAlign::Center),
                    );
                });
                receiver
                    .recv()
                    .expect("Sender was dropped without sending anything")
            };
            // <HACK> I'm not sure how to do it better...
            let source_value = self.to_map().get(&path).cloned();
            let to_patch = match (source_value, choice) {
                (Some(mut value), Some(choice)) => {
                    debug!(
                        "Attempt to replace {:?} with {:?} on path {:?}",
                        value, choice, path
                    );
                    value
                        .parse_replace(&choice)
                        .expect("Invalid value provided as resolve");
                    ItemChange::Set(value)
                }
                // This will fail if some non-string value is added...
                (None, Some(choice)) => ItemChange::Set(GameDataValue::String(choice)),
                (_, None) => ItemChange::Removed,
            };
            out.insert(path, to_patch);
        }

        out
    }
}

impl BTreePatchable for HeroOverride {
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        debug!("{:?}", patch);
        todo!("Applying patch to hero override");
    }
    fn try_merge_patches(
        &self,
        _patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Conflicts) {
        todo!("Merging patches to hero override");
    }
    fn ask_for_resolve(&self, _sink: &mut cursive::CbSink, _patches: Conflicts) -> Patch {
        todo!("Resolving conflicts on hero override");
    }
}

impl Loadable for HeroInfo {
    fn prepare_list(root_path: &std::path::Path) -> io::Result<Vec<std::path::PathBuf>> {
        let path = root_path.join("heroes");
        if path.exists() {
            collect_paths(&path, |path| Ok(ends_with(path, ".info.darkest")))
        } else {
            Ok(vec![])
        }
    }
    fn load_raw(path: &std::path::Path) -> io::Result<Self> {
        let id = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .next()
            .unwrap()
            .to_string();

        let darkest_file = std::fs::read_to_string(path)?;
        let (darkest_file, rest) = darkest_parser()
            .easy_parse(darkest_file.as_str())
            .map_err(|err| err_context(err, &darkest_file))
            .unwrap();
        debug_assert_eq!(rest, "");

        // OK, now let's get these parts out...
        let mut resistances = None;
        let mut weapons = vec![];
        let mut armours = vec![];
        let mut skills = vec![];
        let mut riposte_skill = vec![];
        let mut move_skill = vec![];
        let mut tags = vec![];
        let mut extra_stack_limit = vec![];
        let mut deaths_door = None;
        let mut modes = vec![];
        let mut incompatible_party_member = vec![];
        let mut unparsed = vec![];
        let mut other = HashMap::new();

        for (key, entry) in darkest_file {
            match key.as_str() {
                "resistances" => {
                    let existing = resistances.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "weapon" => weapons.push(entry),
                "armour" => armours.push(entry),
                "combat_skill" => skills.push(entry),
                "riposte_skill" => riposte_skill.push(entry),
                "combat_move_skill" => move_skill.push(entry),
                "tag" => tags.extend(entry.get("id").cloned().unwrap()),
                "extra_stack_limit" => extra_stack_limit.extend(entry.get("id").cloned().unwrap()),
                "deaths_door" => {
                    let existing = deaths_door.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "mode" => modes.push(entry),
                "incompatible_party_member" => incompatible_party_member.push(entry),
                "death_reaction"
                | "hp_reaction"
                | "overstressed_modifier"
                | "extra_battle_loot" => unparsed.push((key, entry)),
                _ => {
                    for (subkey, values) in entry {
                        let existing = other.insert((key.clone(), subkey), values);
                        debug_assert!(existing.is_none());
                    }
                }
            }
        }
        Ok(Self {
            id,
            resistances: Resistances::from_entry(resistances.unwrap()),
            weapons: Weapons::from_entries(weapons),
            armours: Armours::from_entries(armours),
            skills: Skills::from_entries(skills),
            riposte_skill: Skill::try_from_entries(riposte_skill),
            move_skill: Skill::from_entries(move_skill),
            tags,
            extra_stack_limit,
            deaths_door: DeathsDoor::from_entry(deaths_door.unwrap()),
            modes: Modes::from_entries(modes),
            incompatible_party_member: Incompatibilities::from_entries(incompatible_party_member),
            unparsed: Unparsed::from_entries(unparsed),
            other,
        })
    }
}

impl Loadable for HeroOverride {
    fn prepare_list(root_path: &std::path::Path) -> io::Result<Vec<std::path::PathBuf>> {
        let path = root_path.join("heroes");
        if path.exists() {
            collect_paths(&path, |path| Ok(ends_with(path, ".override.darkest")))
        } else {
            Ok(vec![])
        }
    }
    fn load_raw(path: &std::path::Path) -> io::Result<Self> {
        let id = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .next()
            .unwrap()
            .to_string();

        let darkest_file = std::fs::read_to_string(path)?;
        let (darkest_file, rest) = darkest_parser()
            .easy_parse(darkest_file.as_str())
            .map_err(|err| err_context(err, &darkest_file))
            .unwrap();
        debug_assert_eq!(rest, "");

        // OK, now let's get these parts out...
        let mut resistances = None;
        let mut weapons = vec![];
        let mut armours = vec![];
        let mut skills = vec![];
        let mut riposte_skill = vec![];
        let mut move_skill = vec![];
        let mut tags = vec![];
        let mut extra_stack_limit = vec![];
        let mut deaths_door = None;
        let mut modes = vec![];
        let mut incompatible_party_member = vec![];
        let mut unparsed = vec![];
        let mut other = HashMap::new();

        for (key, entry) in darkest_file {
            match key.as_str() {
                "resistances" => {
                    let existing = resistances.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "weapon" => weapons.push(entry),
                "armour" => armours.push(entry),
                "combat_skill" => skills.push(entry),
                "riposte_skill" => riposte_skill.push(entry),
                "combat_move_skill" => move_skill.push(entry),
                "tag" => tags.extend(entry.get("id").cloned().unwrap()),
                "extra_stack_limit" => extra_stack_limit.extend(entry.get("id").cloned().unwrap()),
                "deaths_door" => {
                    let existing = deaths_door.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "mode" => modes.push(entry),
                "incompatible_party_member" => incompatible_party_member.push(entry),
                "death_reaction"
                | "hp_reaction"
                | "overstressed_modifier"
                | "extra_battle_loot" => unparsed.push((key, entry)),
                _ => {
                    for (subkey, values) in entry {
                        let existing = other.insert((key.clone(), subkey), values);
                        debug_assert!(existing.is_none());
                    }
                }
            }
        }
        Ok(Self {
            id,
            resistances: resistances
                .map(ResistancesOverride::from_entry)
                .unwrap_or_default(),
            weapons: opt_vec(weapons).map(Weapons::from_entries),
            armours: opt_vec(armours).map(Armours::from_entries),
            skills: opt_vec(skills).map(Skills::from_entries),
            riposte_skill: opt_vec(riposte_skill).map(Skill::from_entries),
            move_skill: opt_vec(move_skill).map(Skill::from_entries),
            tags,
            extra_stack_limit,
            deaths_door: deaths_door.map(DeathsDoor::from_entry),
            modes: opt_vec(modes).map(Modes::from_entries),
            incompatible_party_member: opt_vec(incompatible_party_member)
                .map(Incompatibilities::from_entries),
            unparsed: Unparsed::from_entries(unparsed),
            other,
        })
    }
}

fn opt_vec<T>(v: Vec<T>) -> Option<Vec<T>> {
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

impl DeployableStructured for HeroInfo {
    fn deploy(&self, path: &std::path::Path) -> io::Result<()> {
        let mut output = File::create(path)?;
        writeln!(output, "// Deployed by Darkest Dungeon Mod Bundler\n")?;

        self.resistances.deploy(&mut output)?;
        self.weapons.deploy(&mut output, &self.id)?;
        self.armours.deploy(&mut output, &self.id)?;
        self.skills.deploy(&mut output)?;
        if let Some(riposte_skill) = &self.riposte_skill {
            riposte_skill.deploy(&mut output)?;
        }
        self.move_skill.deploy(&mut output)?;
        if !self.tags.is_empty() {
            writeln!(output, "// Hero tags")?;
            for tag in &self.tags {
                writeln!(output, "tag: .id {}", tag)?;
            }
            writeln!(output)?;
        }
        if !self.extra_stack_limit.is_empty() {
            writeln!(output, "// Extra stack limits provided by hero")?;
            for extra_limit in &self.extra_stack_limit {
                writeln!(output, "extra_stack_limit: .id {}", extra_limit)?;
            }
            writeln!(output)?;
        }
        self.deaths_door.deploy(&mut output)?;
        self.modes.deploy(&mut output)?;
        self.incompatible_party_member.deploy(&mut output)?;
        self.unparsed.deploy(&mut output)?;
        if !self.other.is_empty() {
            writeln!(output, "// Unclassified hero info")?;
            // TODO - maybe change internal format?..
            let mut other_entries: BTreeMap<_, BTreeMap<_, _>> = BTreeMap::new();
            for ((entry, key), values) in &self.other {
                other_entries
                    .entry(entry.clone())
                    .or_default()
                    .insert(key.clone(), values.join(" "));
            }
            for (entry, values) in other_entries {
                write!(output, "{}: ", entry)?;
                for (key, value) in values {
                    write!(output, ".{} {} ", key, value)?;
                }
                writeln!(output)?;
            }
            writeln!(output)?;
        }
        Ok(())
    }
}

impl DeployableStructured for HeroOverride {
    fn deploy(&self, path: &std::path::Path) -> io::Result<()> {
        let mut output = File::create(path)?;
        writeln!(output, "// Deployed by Darkest Dungeon Mod Bundler\n\n")?;

        self.resistances.deploy(&mut output)?;
        if let Some(weapons) = &self.weapons {
            weapons.deploy(&mut output, &self.id)?;
        }
        if let Some(armours) = &self.armours {
            armours.deploy(&mut output, &self.id)?;
        }
        if let Some(skills) = &self.skills {
            skills.deploy(&mut output)?;
        }
        if let Some(riposte_skill) = &self.riposte_skill {
            riposte_skill.deploy(&mut output)?;
        }
        if let Some(move_skill) = &self.move_skill {
            move_skill.deploy(&mut output)?;
        }
        if !self.tags.is_empty() {
            writeln!(output, "// Hero tags")?;
            for tag in &self.tags {
                writeln!(output, "tag: .id {}", tag)?;
            }
            writeln!(output)?;
        }
        if !self.extra_stack_limit.is_empty() {
            writeln!(output, "// Extra stack limits provided by hero")?;
            for extra_limit in &self.extra_stack_limit {
                writeln!(output, "extra_stack_limit: .id {}", extra_limit)?;
            }
            writeln!(output)?;
        }
        if let Some(deaths_door) = &self.deaths_door {
            deaths_door.deploy(&mut output)?;
        }
        if let Some(modes) = &self.modes {
            modes.deploy(&mut output)?;
        }
        if let Some(incompatible_party_member) = &self.incompatible_party_member {
            incompatible_party_member.deploy(&mut output)?;
        }
        self.unparsed.deploy(&mut output)?;
        if !self.other.is_empty() {
            writeln!(output, "// Unclassified hero info")?;
            // TODO - maybe change internal format to this and not recode on deploy?..
            let mut other_entries: BTreeMap<_, BTreeMap<_, _>> = BTreeMap::new();
            for ((entry, key), values) in &self.other {
                other_entries
                    .entry(entry.clone())
                    .or_default()
                    .insert(key.clone(), values.join(" "));
            }
            for (entry, values) in other_entries {
                write!(output, "{}: ", entry)?;
                for (key, value) in values {
                    write!(output, ".{} {} ", key, value)?;
                }
                writeln!(output)?;
            }
            writeln!(output)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Resistances {
    stun: f32,
    poison: f32,
    bleed: f32,
    disease: f32,
    moving: f32,
    debuff: f32,
    death_blow: f32,
    trap: f32,
}

impl Resistances {
    fn from_entry(mut input: DarkestEntry) -> Self {
        macro_rules! extract {
            ($($key:literal -> $ident:ident),+) => {
                $(
                    let $ident = input.remove($key).unwrap_or_else(|| panic!("Malformed hero information file, no {} resistance found", $key));
                    assert_eq!($ident.len(), 1, "Malformed hero information file: {} resistance have multiple values", $key);
                    let $ident = parse_percent(&$ident[0]).unwrap_or_else(|_| panic!("Malformed hero information file, {} resistance is not an percent-like value", $key));
                )+
            };
        }
        extract!(
            "stun" -> stun,
            "poison" -> poison,
            "bleed" -> bleed,
            "disease" -> disease,
            "move" -> moving,
            "debuff" -> debuff,
            "death_blow" -> death_blow,
            "trap" -> trap
        );
        assert!(input.is_empty());
        Self {
            stun,
            poison,
            bleed,
            disease,
            moving,
            debuff,
            death_blow,
            trap,
        }
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "resistances");
        assert!(path.len() == 2, "Invalid path: {:?}", path);
        let num = change
            .into_option()
            .unwrap_or_else(|| {
                panic!(
                    "Unexpected patch for resistances: trying to remove key {:?}",
                    path
                )
            })
            .unwrap_f32();
        *(match path[1].as_str() {
            "stun" => &mut self.stun,
            "poison" => &mut self.poison,
            "bleed" => &mut self.bleed,
            "disease" => &mut self.disease,
            "move" => &mut self.moving,
            "debuff" => &mut self.debuff,
            "death_blow" => &mut self.death_blow,
            "trap" => &mut self.trap,
            _ => panic!("Unexpected patch for resistances: key = {:?}", path),
        }) = num;
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        writeln!(
            target,
            "resistances: .stun {} .poison {} .bleed {} .disease {} .move {} .debuff {} .death_blow {} .trap {}\n", 
            percent_to_string(self.stun),
            percent_to_string(self.poison),
            percent_to_string(self.bleed),
            percent_to_string(self.disease),
            percent_to_string(self.moving),
            percent_to_string(self.debuff),
            percent_to_string(self.death_blow),
            percent_to_string(self.trap),
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct ResistancesOverride {
    stun: Option<f32>,
    poison: Option<f32>,
    bleed: Option<f32>,
    disease: Option<f32>,
    moving: Option<f32>,
    debuff: Option<f32>,
    death_blow: Option<f32>,
    trap: Option<f32>,
}

impl ResistancesOverride {
    fn from_entry(input: DarkestEntry) -> Self {
        macro_rules! extract {
            ($($key:literal -> $ident:ident),+) => {
                $(
                    let $ident = input.get($key).map(|data| {
                        assert_eq!(data.len(), 1, "Malformed hero information file: {} resistance have multiple values", $key);
                        parse_percent(&data[0]).unwrap_or_else(|_| panic!("Malformed hero information file, {} resistance is not an percent-like value", $key))
                    });
                )+
            };
        }
        extract!(
            "stun" -> stun,
            "poison" -> poison,
            "bleed" -> bleed,
            "disease" -> disease,
            "move" -> moving,
            "debuff" -> debuff,
            "death_blow" -> death_blow,
            "trap" -> trap
        );
        Self {
            stun,
            poison,
            bleed,
            disease,
            moving,
            debuff,
            death_blow,
            trap,
        }
    }
    #[allow(dead_code)] // to be used in HeroOverride
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "resistances");
        assert!(path.len() == 2, "Invalid path: {:?}", path);
        *(match path[1].as_str() {
            "stun" => &mut self.stun,
            "poison" => &mut self.poison,
            "bleed" => &mut self.bleed,
            "disease" => &mut self.disease,
            "move" => &mut self.moving,
            "debuff" => &mut self.debuff,
            "death_blow" => &mut self.death_blow,
            "trap" => &mut self.trap,
            _ => panic!("Unexpected patch for resistances: key = {:?}", path),
        }) = change.into_option().map(GameDataValue::unwrap_f32);
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        let stun = self.stun.map(percent_to_string);
        let poison = self.poison.map(percent_to_string);
        let bleed = self.bleed.map(percent_to_string);
        let disease = self.disease.map(percent_to_string);
        let moving = self.moving.map(percent_to_string);
        let debuff = self.debuff.map(percent_to_string);
        let death_blow = self.death_blow.map(percent_to_string);
        let trap = self.trap.map(percent_to_string);
        if stun.is_none()
            && poison.is_none()
            && bleed.is_none()
            && disease.is_none()
            && moving.is_none()
            && debuff.is_none()
            && death_blow.is_none()
            && trap.is_none()
        {
            return Ok(());
        }
        write!(target, "resistances: ")?;
        if let Some(stun) = stun {
            write!(target, " .stun {} ", stun)?;
        }
        if let Some(poison) = poison {
            write!(target, " .poison {} ", poison)?;
        }
        if let Some(bleed) = bleed {
            write!(target, " .bleed {} ", bleed)?;
        }
        if let Some(disease) = disease {
            write!(target, " .disease {} ", disease)?;
        }
        if let Some(moving) = moving {
            write!(target, " .moving {} ", moving)?;
        }
        if let Some(debuff) = debuff {
            write!(target, " .debuff {} ", debuff)?;
        }
        if let Some(death_blow) = death_blow {
            write!(target, " .death_blow {} ", death_blow)?;
        }
        if let Some(trap) = trap {
            write!(target, " .trap {} ", trap)?;
        }
        writeln!(target)?;
        Ok(())
    }
}

impl BTreeMappable for Resistances {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.insert(vec!["stun".into()], self.stun.into());
        out.insert(vec!["poison".into()], self.poison.into());
        out.insert(vec!["bleed".into()], self.bleed.into());
        out.insert(vec!["disease".into()], self.disease.into());
        out.insert(vec!["move".into()], self.moving.into());
        out.insert(vec!["debuff".into()], self.debuff.into());
        out.insert(vec!["death_blow".into()], self.death_blow.into());
        out.insert(vec!["trap".into()], self.trap.into());
        out
    }
}

impl BTreeMappable for ResistancesOverride {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        if let Some(stun) = self.stun {
            out.insert(vec!["stun".into()], stun.into());
        }
        if let Some(poison) = self.poison {
            out.insert(vec!["poison".into()], poison.into());
        }
        if let Some(bleed) = self.bleed {
            out.insert(vec!["bleed".into()], bleed.into());
        }
        if let Some(disease) = self.disease {
            out.insert(vec!["disease".into()], disease.into());
        }
        if let Some(moving) = self.moving {
            out.insert(vec!["move".into()], moving.into());
        }
        if let Some(debuff) = self.debuff {
            out.insert(vec!["debuff".into()], debuff.into());
        }
        if let Some(death_blow) = self.death_blow {
            out.insert(vec!["death_blow".into()], death_blow.into());
        }
        if let Some(trap) = self.trap {
            out.insert(vec!["trap".into()], trap.into());
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Weapons([Weapon; 5]);
#[derive(Clone, Debug, Default)]
struct Weapon {
    atk: f32,
    dmg: (i32, i32),
    crit: f32,
    spd: i32,
}

impl Weapons {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let out: Vec<_> = input.into_iter().map(Weapon::from_entry).collect();
        let out: &[_; 5] = out
            .as_slice()
            .try_into()
            .expect("Should be exactly 5 weapons");
        Self(out.to_owned())
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "weapons");
        let index: usize = path[1].parse().unwrap();
        match path[2].as_str() {
            "atk" => self.0[index].atk = change.unwrap_set().unwrap_f32(),
            "dmg min" => self.0[index].dmg.0 = change.unwrap_set().unwrap_i32(),
            "dmg max" => self.0[index].dmg.1 = change.unwrap_set().unwrap_i32(),
            "crit" => self.0[index].atk = change.unwrap_set().unwrap_f32(),
            "spd" => self.0[index].spd = change.unwrap_set().unwrap_i32(),
            _ => panic!("Unexpected key in hero into patch: {:?}", path),
        };
    }
    fn deploy(&self, target: &mut File, hero_id: &str) -> io::Result<()> {
        writeln!(target, "// Weapons\n")?;
        writeln!(
            target,
            "weapon: .name \"{}_weapon_0\" {}",
            hero_id,
            self.0[0].to_string()
        )?;
        writeln!(
            target,
            "weapon: .name \"{}_weapon_1\" {} .upgradeRequirementCode 0",
            hero_id,
            self.0[1].to_string()
        )?;
        writeln!(
            target,
            "weapon: .name \"{}_weapon_2\" {} .upgradeRequirementCode 1",
            hero_id,
            self.0[2].to_string()
        )?;
        writeln!(
            target,
            "weapon: .name \"{}_weapon_3\" {} .upgradeRequirementCode 2",
            hero_id,
            self.0[3].to_string()
        )?;
        writeln!(
            target,
            "weapon: .name \"{}_weapon_4\" {} .upgradeRequirementCode 3",
            hero_id,
            self.0[4].to_string()
        )?;
        writeln!(target)?;
        Ok(())
    }
}

impl Weapon {
    fn from_entry(input: DarkestEntry) -> Self {
        let mut out = Self::default();
        out.atk = parse_percent(
            input
                .get("atk")
                .expect("Weapon ATK not found")
                .get(0)
                .expect("Weapon ATK field is empty"),
        )
        .expect("Weapon ATK is not a number");
        let mut dmg = input
            .get("dmg")
            .expect("Weapon DMG field not found")
            .iter()
            .map(|s| s.parse().expect("Weapon DMG field is not a number"));
        out.dmg = (
            dmg.next().expect("Weapon DMG field is empty"),
            dmg.next().expect("Weapon DMG field has only one entry"),
        );
        out.crit = parse_percent(&input.get("crit").expect("Weapon CRIT field not found")[0])
            .expect("Weapon CRIT field is not a number");
        let spd = input
            .get("spd")
            .expect("Weapon SPD field not found")
            .get(0)
            .expect("Weapon SPD field is empty");
        out.spd = spd.parse().expect("Weapon SPD field is not a number");
        out
    }
}
impl Display for Weapon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            ".atk {} .dmg {} {} .crit {} .spd {}",
            percent_to_string(self.atk),
            self.dmg.0,
            self.dmg.1,
            percent_to_string(self.crit),
            self.spd
        )
    }
}

impl BTreeMappable for Weapons {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (index, item) in self.0.iter().enumerate() {
            out.extend_prefixed(&index.to_string(), item.to_map());
        }
        out
    }
}

impl BTreeMappable for Weapon {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.insert(vec!["atk".into()], self.atk.into());
        out.insert(vec!["dmg min".into()], self.dmg.0.into());
        out.insert(vec!["dmg max".into()], self.dmg.1.into());
        out.insert(vec!["crit".into()], self.crit.into());
        out.insert(vec!["spd".into()], self.spd.into());
        out
    }
}

#[derive(Clone, Debug)]
struct Armours([Armour; 5]);
#[derive(Clone, Debug, Default)]
struct Armour {
    def: f32,
    prot: f32,
    hp: i32,
    spd: i32,
}

impl Armours {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let out: Vec<_> = input.into_iter().map(Armour::from_entry).collect();
        let out: &[_; 5] = out
            .as_slice()
            .try_into()
            .expect("Should be exactly 5 armours");
        Self(out.to_owned())
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "armours");
        let index: usize = path[1].parse().unwrap();
        match path[2].as_str() {
            "def" => self.0[index].def = change.unwrap_set().unwrap_f32(),
            "prot" => self.0[index].prot = change.unwrap_set().unwrap_f32(),
            "hp" => self.0[index].hp = change.unwrap_set().unwrap_i32(),
            "spd" => self.0[index].spd = change.unwrap_set().unwrap_i32(),
            _ => panic!("Unexpected key in hero into patch: {:?}", path),
        };
    }
    fn deploy(&self, target: &mut File, hero_id: &str) -> io::Result<()> {
        writeln!(target, "// Armours")?;
        writeln!(
            target,
            "armour: .name \"{}_armour_0\" {}",
            hero_id,
            self.0[0].to_string()
        )?;
        writeln!(
            target,
            "armour: .name \"{}_armour_1\" {} .upgradeRequirementCode 0",
            hero_id,
            self.0[1].to_string()
        )?;
        writeln!(
            target,
            "armour: .name \"{}_armour_2\" {} .upgradeRequirementCode 1",
            hero_id,
            self.0[2].to_string()
        )?;
        writeln!(
            target,
            "armour: .name \"{}_armour_3\" {} .upgradeRequirementCode 2",
            hero_id,
            self.0[3].to_string()
        )?;
        writeln!(
            target,
            "armour: .name \"{}_armour_4\" {} .upgradeRequirementCode 3",
            hero_id,
            self.0[4].to_string()
        )?;
        writeln!(target)?;
        Ok(())
    }
}
impl Armour {
    fn from_entry(input: DarkestEntry) -> Self {
        let mut out = Self::default();
        out.def = parse_percent(&input.get("def").expect("Armour DEF field not found")[0])
            .expect("Armour DEF field is not a number");
        out.prot = parse_percent(&input.get("prot").expect("Armour PROT field not found")[0])
            .expect("Armour PROT field is not a number");
        out.hp = input.get("hp").expect("Armour HP field not found")[0]
            .parse()
            .expect("Armour HP field is not a number");
        out.spd = input.get("spd").expect("Armour SPD field not found")[0]
            .parse()
            .expect("Armour SPD field is not a number");
        out
    }
}
impl Display for Armour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            ".def {} .prot {} .hp {} .spd {}",
            percent_to_string(self.def),
            self.prot,
            self.hp,
            self.spd
        )
    }
}

impl BTreeMappable for Armours {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (index, item) in self.0.iter().enumerate() {
            out.extend_prefixed(&index.to_string(), item.to_map());
        }
        out
    }
}

impl BTreeMappable for Armour {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.insert(vec!["def".into()], self.def.into());
        out.insert(vec!["prot".into()], self.prot.into());
        out.insert(vec!["hp".into()], self.hp.into());
        out.insert(vec!["spd".into()], self.spd.into());
        out
    }
}

#[derive(Clone, Debug)]
struct Skills(BTreeMap<String, BTreeMap<i32, Skill>>);

impl Skills {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let mut tmp: HashMap<String, HashMap<i32, Vec<DarkestEntry>>> = HashMap::new();
        for entry in input {
            let id = entry.get("id").expect("Skill ID field not found")[0].clone();
            let level = entry.get("level").expect("Skill LEVEL field not found")[0]
                .parse()
                .expect("Skill LEVEL field is not a number");
            tmp.entry(id)
                .or_default()
                .entry(level)
                .or_default()
                .push(entry);
        }
        Self(
            tmp.into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        value
                            .into_iter()
                            .map(|(key, value)| (key, Skill::from_entries(value)))
                            .collect(),
                    )
                })
                .collect(),
        )
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "skills");
        let name = &path[1];
        let level = path[2].parse().unwrap_or_else(|_| {
            panic!(
                "Unexpected path in hero data: {:?}, wrong skill level",
                path
            )
        });
        assert!(level >= 0 && level < 5);
        self.0
            .get_mut(name)
            .unwrap_or_else(|| panic!("Unexpected path in hero data: {:?}, skill not found", path))
            .entry(level)
            .or_default()
            .apply(path, change);
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        for (id, skill) in &self.0 {
            writeln!(target, "// Skill: {}", id)?;
            for skill in skill.values() {
                writeln!(target, "combat_skill: {}", skill.to_string())?;
            }
            writeln!(target)?;
        }
        Ok(())
    }
}

impl BTreeMappable for Skills {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (name, skill) in &self.0 {
            let mut skill_map = DataMap::new();
            for (level, skill) in skill {
                let map = skill.to_map();
                skill_map.extend_prefixed(&level.to_string(), map);
            }
            out.extend_prefixed(&name, skill_map);
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
struct Skill {
    id: String,
    level: i32,
    effects: Vec<String>,
    other: BTreeMap<String, String>,
}

impl Skill {
    fn try_from_entries(input: Vec<DarkestEntry>) -> Option<Self> {
        if input.is_empty() {
            None
        } else {
            Some(Self::from_entries(input))
        }
    }
    fn from_entries(mut input: Vec<DarkestEntry>) -> Self {
        let effects = input
            .iter_mut()
            .flat_map(|entry| entry.remove("effect").unwrap_or_default())
            .collect();
        let mut other: BTreeMap<_, _> = input
            .into_iter()
            .flat_map(|entry| entry.into_iter())
            .map(|(key, v)| (key, v.join(" ")))
            .collect();
        let id = other.remove("id").unwrap();
        let level = other
            .remove("level")
            .unwrap()
            .parse()
            .expect("Malformed hero info file - wrong skill level format");
        Self {
            id,
            level,
            effects,
            other,
        }
    }
    fn get(&self, subpath: &[String]) -> String {
        match subpath[0].as_str() {
            "effects" => self.effects.clone().join(" "),
            "other" => self.other.get(&subpath[1]).cloned().unwrap_or_default(),
            _ => panic!("Unexpected path in skill info: {:?}", subpath),
        }
    }
    fn apply(&mut self, mut path: Vec<String>, change: ItemChange) {
        debug!("Patching skill: path = {:?}, change = {:?}", path, change);
        match path[0].clone().as_str() {
            "skills" => {
                // Drop skill and its level from the path.
                let _ = path.drain(1..=2);
            }
            "riposte_skill" => (),
            _ => panic!("Unexpected path in hero info: {:?}", path),
        };
        match path[1].as_str() {
            "effects" => {
                assert!(path.len() == 2);
                self.effects = change
                    .into_option()
                    .unwrap()
                    .unwrap_string()
                    .split('\n')
                    .map(String::from)
                    .collect();
            }
            "other" => {
                assert!(path.len() == 3);
                match change.into_option().map(GameDataValue::unwrap_string) {
                    Some(s) => self.other.insert(path.remove(2), s),
                    None => self.other.remove(&path.remove(2)),
                };
            }
            _ => panic!("Unexpected path in hero info: {:?}", path),
        };
    }
    // TODO - this can be misused
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        writeln!(target, "// Riposte Skill")?;
        writeln!(target, "riposte_skill: {}", self)?;
        writeln!(target)?;
        Ok(())
    }
}
impl Display for Skill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let effects = if self.effects.is_empty() {
            "".into()
        } else {
            String::from(".effect ") + &self.effects.join(" ")
        };
        let other = self
            .other
            .iter()
            .map(|(key, value)| format!(".{} {}", key, value))
            .collect::<Vec<_>>()
            .join(" ");
        write!(
            f,
            " .id {} .level {} {} {}",
            self.id, self.level, other, effects
        )
    }
}

impl BTreeMappable for Skill {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.insert(vec!["effects".into()], self.effects.join("\n").into());
        out.extend_prefixed(
            "other",
            self.other
                .clone()
                .into_iter()
                .map(|(key, value)| (vec![key], value.into())),
        );
        out
    }
}

#[derive(Clone, Debug)]
struct DeathsDoor {
    buffs: Vec<String>,
    recovery_buffs: Vec<String>,
    recovery_heart_attack_buffs: Vec<String>,
}

impl DeathsDoor {
    fn from_entry(mut input: DarkestEntry) -> Self {
        Self {
            buffs: input.remove("buffs").unwrap_or_default(),
            recovery_buffs: input.remove("recovery_buffs").unwrap_or_default(),
            recovery_heart_attack_buffs: input
                .remove("recovery_heart_attack_buffs")
                .unwrap_or_default(),
        }
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        assert!(path.len() == 3);
        debug_assert_eq!(path[0], "deaths_door");
        let place = match path[1].as_str() {
            "buffs" => &mut self.buffs,
            "recovery_buffs" => &mut self.recovery_buffs,
            "recovery_heart_attack_buffs" => &mut self.recovery_heart_attack_buffs,
            _ => panic!("Unexpected key in hero info patch: {:?}", path),
        };
        patch_list(place, path, change);
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        writeln!(target, "// Death's Door Effects")?;
        writeln!(
            target,
            "deaths_door: .buffs {} .recovery_buffs {} .recovery_heart_attack_buffs {}",
            self.buffs.join(" "),
            self.recovery_buffs.join(" "),
            self.recovery_heart_attack_buffs.join(" ")
        )?;
        writeln!(target)?;
        Ok(())
    }
}

impl BTreeMappable for DeathsDoor {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.extend_prefixed("buffs", self.buffs.to_set());
        out.extend_prefixed("recovery_buffs", self.recovery_buffs.to_set());
        out.extend_prefixed(
            "recovery_heart_attack_buffs",
            self.recovery_heart_attack_buffs.to_set(),
        );
        out
    }
}

#[derive(Clone, Debug)]
struct Modes(HashMap<String, Mode>);
impl Modes {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        Self(input.into_iter().map(Mode::from_entry).collect())
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "modes");
        self.0
            .get_mut(path.get(1).unwrap())
            .unwrap_or_else(|| panic!("Unexpected path in hero data: {:?}, mode not found", path))
            .apply(path, change);
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        writeln!(target, "// Hero combat modes")?;
        for (key, mode) in &self.0 {
            writeln!(target, "mode: .id {} {}", key, mode.to_string())?;
        }
        writeln!(target)?;
        Ok(())
    }
}

impl BTreeMappable for Modes {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (key, value) in &self.0 {
            out.extend_prefixed(key, value.to_map());
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Mode(HashMap<String, Vec<String>>);
impl Mode {
    fn from_entry(mut input: DarkestEntry) -> (String, Self) {
        (
            input.remove("id").unwrap().remove(0),
            Self(input.into_iter().collect()),
        )
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "modes");
        assert!(path.len() == 4);
        let place = self.0.entry(path.get(2).unwrap().clone()).or_default();
        patch_list(place, path, change);
    }
}
impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO
        self.0
            .iter()
            .map(|(key, values)| format!(" .{} {} ", key, values.join(" ")))
            .collect::<Vec<_>>()
            .join(" ")
            .fmt(f)
    }
}

impl BTreeMappable for Mode {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (key, value) in &self.0 {
            out.extend_prefixed(key, value.to_set());
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Incompatibilities(HashMap<String, Vec<String>>);
impl Incompatibilities {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let mut map = HashMap::new();
        for mut entry in input {
            let id = entry.remove("id").unwrap().remove(0);
            let tag = entry.remove("hero_tag").unwrap().remove(0);
            // False positive from clippy - https://github.com/rust-lang/rust-clippy/issues/5693
            #[allow(clippy::or_fun_call)]
            map.entry(id).or_insert(vec![]).push(tag);
        }
        Self(map)
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "incompatible_party_member");
        assert!(path.len() == 3);
        let place = self.0.entry(path.get(1).unwrap().clone()).or_default();
        patch_list(place, path, change);
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        writeln!(target, "// Rules for party incompatibilities")?;
        for (id, tags) in &self.0 {
            for tag in tags {
                writeln!(target, "incompatible_party_member: .id {} .tag {}", id, tag)?;
            }
        }
        writeln!(target)?;
        Ok(())
    }
}

impl BTreeMappable for Incompatibilities {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (key, value) in &self.0 {
            out.extend_prefixed(key, value.to_set());
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Unparsed(HashMap<String, Vec<String>>);
impl Unparsed {
    fn from_entries(input: Vec<(String, DarkestEntry)>) -> Self {
        let mut inner: HashMap<_, Vec<_>> = HashMap::new();
        for (key, entry) in input {
            inner.entry(key).or_default().push(entry.to_string());
        }
        Self(inner)
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        assert!(path.len() == 3);
        patch_list(self.0.entry(path[1].clone()).or_default(), path, change);
    }
    fn deploy(&self, target: &mut File) -> io::Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        writeln!(target, "// Unparsed entries")?;
        for (key, entries) in &self.0 {
            for entry in entries {
                writeln!(target, "{}: {}", key, entry)?;
            }
        }
        writeln!(target)?;
        Ok(())
    }
}

impl BTreeMappable for Unparsed {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for (key, value) in &self.0 {
            out.extend_prefixed(&key, value.to_set());
        }
        out
    }
}
