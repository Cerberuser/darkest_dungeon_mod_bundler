use crate::bundler::{
    diff::{self, Conflicts, DataMap, ItemChange, Patch},
    game_data::{
        file_types::{darkest_parser, DarkestEntry},
        BTreeMapExt, BTreeMappable, BTreePatchable, BTreeSetable, GameDataValue, Loadable,
    },
    loader::utils::{collect_paths, ends_with},
    ModFileChange,
};
use combine::EasyParser;
use crossbeam_channel::{bounded, Sender};
use cursive::{
    traits::{Nameable, Resizable},
    views::{Button, Dialog, EditView, LinearLayout, Panel, TextArea, TextView},
};
use log::debug;
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryInto,
    num::ParseFloatError,
    ops::Deref,
};

fn parse_percent(value: &str) -> Result<f32, ParseFloatError> {
    if value.ends_with('%') {
        Ok(value.trim_end_matches('%').parse::<f32>()? / 100.0)
    } else {
        value.parse()
    }
}

#[derive(Clone, Debug)]
pub struct HeroInfo {
    id: String,
    resistances: Resistances,
    weapons: Weapons,
    armours: Armours,
    skills: Skills,
    riposte_skill: Skill,
    move_skill: MoveSkill,
    tags: Vec<String>,
    extra_stack_limit: Vec<String>,
    deaths_door: DeathsDoor,
    modes: Modes,
    incompatible_party_member: Incompatibilities,
    death_reaction: DeathReaction,
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
    move_skill: Option<MoveSkill>,
    tags: Vec<String>,
    extra_stack_limit: Vec<String>,
    deaths_door: Option<DeathsDoor>,
    modes: Option<Modes>,
    incompatible_party_member: Option<Incompatibilities>,
    death_reaction: Option<DeathReaction>,
    other: HashMap<(String, String), Vec<String>>,
}

impl BTreeMappable for HeroInfo {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();

        out.extend_prefixed("resistances", self.resistances.to_map());
        out.extend_prefixed("weapons", self.weapons.to_map());
        out.extend_prefixed("armours", self.armours.to_map());
        out.extend_prefixed("skills", self.skills.to_map());
        out.extend_prefixed("riposte_skill", self.riposte_skill.to_map());
        out.extend_prefixed("move_skill", self.move_skill.to_map());
        out.extend_prefixed("tags", self.tags.to_set());
        out.extend_prefixed("extra_stack_limit", self.extra_stack_limit.to_set());
        out.extend_prefixed("deaths_door", self.deaths_door.to_map());
        out.extend_prefixed("modes", self.modes.to_map());
        out.extend_prefixed(
            "incompatible_party_member",
            self.incompatible_party_member.to_map(),
        );
        out.extend_prefixed("death_reaction", self.death_reaction.to_map());
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
        let mut inner = DataMap::new();

        inner.extend_prefixed("resistances", self.resistances.to_map());
        if let Some(weapons) = &self.weapons {
            inner.extend_prefixed("weapons", weapons.to_map());
        }
        if let Some(armours) = &self.armours {
            inner.extend_prefixed("armours", armours.to_map());
        }
        if let Some(skills) = &self.skills {
            inner.extend_prefixed("skills", skills.to_map());
        }
        if let Some(riposte_skill) = &self.riposte_skill {
            inner.extend_prefixed("riposte_skill", riposte_skill.to_map());
        }
        if let Some(move_skill) = &self.move_skill {
            inner.extend_prefixed("move_skill", move_skill.to_map());
        }
        inner.extend_prefixed("tags", self.tags.to_set());
        inner.extend_prefixed("extra_stack_limit", self.extra_stack_limit.to_set());
        if let Some(deaths_door) = &self.deaths_door {
            inner.extend_prefixed("deaths_door", deaths_door.to_map());
        }
        if let Some(modes) = &self.modes {
            inner.extend_prefixed("modes", modes.to_map());
        }
        if let Some(incompatible_party_member) = &self.incompatible_party_member {
            inner.extend_prefixed(
                "incompatible_party_member",
                incompatible_party_member.to_map(),
            );
        }
        if let Some(death_reaction) = &self.death_reaction {
            inner.extend_prefixed("death_reaction", death_reaction.to_map());
        }
        for (key, value) in &self.other {
            let mut intermid = DataMap::new();
            intermid.extend_prefixed(&key.1, value.to_set());
            let mut intermid_outer = DataMap::new();
            intermid_outer.extend_prefixed(&key.0, intermid);
            inner.extend_prefixed("other", intermid_outer);
        }

        out.extend_prefixed(&self.id, inner);
        out
    }
}

fn next_effect(
    prev: &str,
    orig_effects: &[String],
    patch: &Patch,
    prefix: &[String],
) -> Option<String> {
    let patched = patch.get(
        &prefix
            .iter()
            .cloned()
            .chain(std::iter::once(prev.into()))
            .collect::<Vec<_>>(),
    );
    if let Some(ItemChange::Removed) = &patched {
        // This is set by the patch to be the end of chain.
        return None;
    }
    patched
        .map(|item| match item {
            ItemChange::Set(GameDataValue::String(effect)) => effect,
            _ => panic!("Skill effects can only be strings"),
        })
        .or_else(|| {
            let index = orig_effects.iter().position(|eff| eff == prev);
            index.and_then(|index| orig_effects.get(index + 1))
        })
        .cloned()
}

fn patch_skill_effects(orig_effects: &mut Vec<String>, patch: &Patch, prefix: &[String]) {
    let prefix: Vec<_> = prefix
        .iter()
        .cloned()
        .chain(std::iter::once("effect".into()))
        .collect();
    let mut effects = vec![];
    let start_patched = patch.get(&prefix);
    if let Some(ItemChange::Removed) = &start_patched {
        // Effects are dropped entirely by patch
        *orig_effects = vec![];
        return;
    }
    // Now, there might be some effects; let's find the start of them
    let start = start_patched
        .map(|item| match item {
            ItemChange::Set(GameDataValue::String(effect)) => effect,
            _ => panic!("Skill effects can only be strings"),
        })
        .or_else(|| orig_effects.get(0));
    if let Some(cur) = start {
        let mut cur = cur.to_string();
        // There really are some effects - either patch set them start, or the start remained unchanged
        effects.push(cur.clone());
        while let Some(next) = next_effect(&cur, &*orig_effects, patch, &prefix) {
            effects.push(next.clone());
            cur = next;
        }
        *orig_effects = effects;
    }
    // Otherwise, there were no effects and there are no effects.
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

fn resolve_skill_effects(
    skill_data: &Skill,
    skill_name: String,
    conflicts: &Conflicts,
    prefix: &[String],
    sink: &mut cursive::CbSink,
    self_id: String,
) -> Patch {
    let mut skill_conflicts: Conflicts = conflicts
        .iter()
        .filter(|(key, _)| key.iter().zip(prefix).all(|(key, prefix)| key == prefix))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if skill_conflicts.is_empty() {
        // This skill effects have no problem with it
        return Patch::new();
    }
    debug!(
        "Got conflicts for skill: {} - {:?}",
        skill_name, skill_conflicts
    );
    // Now, we have to build a chain for every file in use.
    let mut conflict_chains: HashMap<String, HashMap<String, Option<String>>> = HashMap::new();
    let mut conflict_starts: HashMap<String, Option<String>> = match skill_conflicts.remove(prefix)
    {
        Some(v) => v
            .into_iter()
            .map(|(mod_name, item)| {
                (
                    mod_name,
                    item.into_option().and_then(GameDataValue::unwrap_list_next),
                )
            })
            .collect(),
        None => HashMap::new(),
    };
    for (mut path, changes) in skill_conflicts {
        assert!(path.len() == 4);
        let key = path.pop().unwrap();
        for (mod_name, change) in changes {
            if let ItemChange::Set(change) = change {
                conflict_chains
                    .entry(mod_name.clone())
                    .or_default()
                    .insert(key.clone(), change.unwrap_list_next());
            }
            conflict_starts
                .entry(mod_name)
                .or_insert_with(|| skill_data.effects.get(0).cloned());
        }
    }
    // Now, we again re-grouped all the changes on per-mod basis; let's build the chains
    let mut chains: BTreeMap<String, Vec<String>> = conflict_starts
        .into_iter()
        .map(|(key, value)| (key, value.into_iter().collect()))
        .collect();

    for (key, steps) in &mut conflict_chains {
        let chain = chains.get_mut(key).unwrap();
        if chain.is_empty() {
            break;
        }
        let mut last = chain.get(0).unwrap().clone();
        loop {
            let next = steps.get(&last).cloned().unwrap_or_else(|| {
                let index = skill_data.effects.iter().position(|eff| eff == &last);
                index.and_then(|index| skill_data.effects.get(index + 1).cloned())
            });
            match next {
                Some(next) => {
                    chain.push(next.clone());
                    last = next;
                }
                None => break,
            }
        }
    }
    debug!("Got effect chains: {:?}", chains);
    // Then, we can check if the resulting chains are really different (it's hard to do before).
    let mut iter = chains.iter();
    let (_, first_chain) = iter.next().unwrap();
    let all_equal = iter.all(|(_, chain)| chain == first_chain);
    if all_equal {
        return diff::diff(skill_data.effects.to_map(), first_chain.to_map());
    }
    // ...and now, finally, ask the user to choose the correct chain.
    // For now, simply as text.
    // let line = skill_data.effects.join(" ");
    // let (sender, receiver) = bounded(0);
    // crate::run_update(sink, move |cursive| {
    //     let mut layout = LinearLayout::vertical();
    //     layout.add_child(
    //         LinearLayout::horizontal()
    //             .child(Panel::new(TextView::new(line.clone()).full_width()).title("Original chain"))
    //             .child(Button::new("Move to input", move |cursive| {
    //                 cursive.call_on_name("Line resolve edit", |edit: &mut TextArea| {
    //                     edit.set_content(line.clone())
    //                 });
    //             })),
    //     );
    //     chains.into_iter().for_each(|(name, line)| {
    //         let line = line.join(" ");
    //         layout.add_child(
    //             LinearLayout::horizontal()
    //                 .child(Panel::new(TextView::new(line.clone()).full_width()).title(name))
    //                 .child(Button::new("Move to input", move |cursive| {
    //                     cursive.call_on_name("Line resolve edit", |edit: &mut TextArea| {
    //                         edit.set_content(line.clone())
    //                     });
    //                 })),
    //         )
    //     });
    //     crate::push_screen(
    //         cursive,
    //         Dialog::around(
    //             layout.child(TextArea::new().with_name("Line resolve edit").full_width()),
    //         )
    //         .title(format!(
    //             "Resolving skill effects: hero ID = {}, skill = {}",
    //             self_id, skill_name
    //         ))
    //         .button("Resolve", move |cursive| {
    //             let value = cursive
    //                 .call_on_name("Line resolve edit", |edit: &mut TextArea| {
    //                     edit.get_content().to_owned()
    //                 })
    //                 .unwrap();
    //             cursive.pop_layer();
    //             sender.send(value).unwrap();
    //         })
    //         .h_align(cursive::align::HAlign::Center),
    //     );
    // });
    // let choice: String = receiver
    //     .recv()
    //     .expect("Sender was dropped without sending anything");
    let choice = "".to_string(); // testing
    let (values, rest) = DarkestEntry::values()
        .easy_parse(choice.as_str())
        .expect("Invalid string given as resolved effects list");
    assert!(rest.is_empty(), "Something was left unparsed: {}", rest);
    diff::diff(skill_data.effects.to_map(), values.to_map())
}

impl BTreePatchable for HeroInfo {
    fn apply_patch(&mut self, patch: Patch) -> Result<(), ()> {
        // First, we should collect all skill effects used in patch.
        for ((skill, level), ref mut skill_data) in self.skills.0.iter_mut() {
            patch_skill_effects(
                &mut skill_data.effects,
                &patch,
                &["skills".into(), format!("{}/{}", skill, level)],
            );
        }
        patch_skill_effects(
            &mut self.riposte_skill.effects,
            &patch,
            &["riposte_skill".into()],
        );

        // Now, all other parts are simpler... we'll just patch it key-by-key.
        for (mut path, change) in patch {
            match path.get(0).unwrap().as_str() {
                "resistances" => self.resistances.apply(path, change),
                "weapons" => self.weapons.apply(path, change),
                "armours" => self.armours.apply(path, change),
                "skills" => self.skills.apply(path, change),
                "riposte_skill" => self.riposte_skill.apply(path, change),
                "move_skill" => self.move_skill.apply(path, change),
                "tags" => patch_list(&mut self.tags, path, change),
                "extra_stack_limit" => patch_list(&mut self.extra_stack_limit, path, change),
                "deaths_door" => self.deaths_door.apply(path, change),
                "modes" => self.modes.apply(path, change),
                "incompatible_party_member" => self.incompatible_party_member.apply(path, change),
                "death_reaction" => self.death_reaction.apply(path, change),
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

        let mut skill_effects: HashMap<String, HashMap<String, Patch>> = HashMap::new();
        let mut riposte_effects: HashMap<String, Patch> = HashMap::new();
        for (path, changes) in changes {
            debug_assert!(!changes.is_empty());
            if path.get(0).map(String::as_str) == Some("skills")
                && path.get(2).map(String::as_str) == Some("effects")
            {
                let cur_map = skill_effects
                    .entry(path.get(1).unwrap().clone())
                    .or_default();
                for (mod_name, change) in changes {
                    cur_map
                        .entry(mod_name)
                        .or_default()
                        .insert(path.clone(), change);
                }
            } else if path.get(0).map(String::as_str) == Some("riposte_skill")
                && path.get(1).map(String::as_str) == Some("effects")
            {
                for (mod_name, change) in changes {
                    riposte_effects
                        .entry(mod_name)
                        .or_default()
                        .insert(path.clone(), change);
                }
            } else if changes.len() == 1 {
                merged.insert(path, changes.into_iter().next().unwrap().1);
            } else {
                for change in changes {
                    unmerged.entry(path.clone()).or_default().push(change);
                }
            }
        }
        for (skill, effects) in skill_effects {
            if effects.is_empty() {
                debug!("Effects for skill {} seem to be non-patched", skill);
            } else if effects.len() == 1 {
                merged.extend(effects.into_iter().next().unwrap().1);
            } else {
                for (mod_name, patch) in effects {
                    for (path, change) in patch {
                        unmerged
                            .entry(path)
                            .or_default()
                            .push((mod_name.clone(), change));
                    }
                }
            }
        }
        if riposte_effects.is_empty() {
            debug!("Riposte skill seems to be non-patched");
        } else if riposte_effects.len() == 1 {
            merged.extend(riposte_effects.into_iter().next().unwrap().1);
        } else {
            for (mod_name, patch) in riposte_effects {
                for (path, change) in patch {
                    unmerged
                        .entry(path)
                        .or_default()
                        .push((mod_name.clone(), change));
                }
            }
        }

        (merged, unmerged)
    }

    fn ask_for_resolve(&self, sink: &mut cursive::CbSink, conflicts: Conflicts) -> Patch {
        let mut out = Patch::new();

        // First, try to merge all conflicts related to skill effects.
        for ((skill, level), skill_data) in self.skills.0.iter() {
            let skill_name = format!("{}/{}", skill, level);
            let prefix = &["skills".into(), skill_name.clone(), "effects".into()] as &[String];
            let effects_patch = resolve_skill_effects(
                skill_data,
                skill_name.clone(),
                &conflicts,
                prefix,
                sink,
                self.id.clone(),
            );
            let skill_patch = {
                let mut patch = Patch::new();
                patch.extend_prefixed("effects", effects_patch);
                patch
            };
            let skill_patch = {
                let mut patch = Patch::new();
                patch.extend_prefixed(&skill_name, skill_patch);
                patch
            };
            out.extend_prefixed("skills", skill_patch);
        }
        // Not to forget about riposte!
        let prefix = &["riposte_skill".into(), "effects".into()] as &[String];
        let effects_patch = resolve_skill_effects(
            &self.riposte_skill,
            "Riposte".into(),
            &conflicts,
            prefix,
            sink,
            self.id.clone(),
        );
        let skill_patch = {
            let mut patch = Patch::new();
            patch.extend_prefixed("effects", effects_patch);
            patch
        };
        out.extend_prefixed("riposte_skill", skill_patch);
        // Now that's easier - we can simply iterate over changes one-by-one.
        for (path, mut changes) in conflicts {
            // Just don't forget that the effects were already dealt with.
            if path[0].as_str() == "skills" && path[2].as_str() == "effects" {
                continue;
            }
            if path[0].as_str() == "riposte_skill" && path[1].as_str() == "effects" {
                continue;
            }
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
        todo!()
    }
    fn try_merge_patches(
        &self,
        patches: impl IntoIterator<Item = ModFileChange>,
    ) -> (Patch, Conflicts) {
        todo!()
    }
    fn ask_for_resolve(&self, sink: &mut cursive::CbSink, patches: Conflicts) -> Patch {
        todo!()
    }
}

impl Loadable for HeroInfo {
    fn prepare_list(root_path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        let path = root_path.join("heroes");
        if path.exists() {
            collect_paths(&path, |path| Ok(ends_with(path, ".info.darkest")))
        } else {
            Ok(vec![])
        }
    }
    fn load_raw(path: &std::path::Path) -> std::io::Result<Self> {
        let id = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .next()
            .unwrap()
            .to_string();

        let darkest_file = std::fs::read_to_string(path)?;
        let (darkest_file, rest) = darkest_parser().easy_parse(darkest_file.as_str()).unwrap();
        debug_assert_eq!(rest, "");

        // OK, now let's get these parts out...
        let mut resistances = None;
        let mut weapons = vec![];
        let mut armours = vec![];
        let mut skills = vec![];
        let mut riposte_skill = vec![];
        let mut move_skill = None;
        let mut tags = vec![];
        let mut extra_stack_limit = vec![];
        let mut deaths_door = None;
        let mut modes = vec![];
        let mut incompatible_party_member = vec![];
        let mut death_reaction = vec![];
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
                "combat_move_skill" => {
                    let existing = move_skill.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "tag" => tags.extend(entry.get("id").cloned().unwrap()),
                "extra_stack_limit" => extra_stack_limit.extend(entry.get("id").cloned().unwrap()),
                "deaths_door" => {
                    let existing = deaths_door.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "mode" => modes.push(entry),
                "incompatible_party_member" => incompatible_party_member.push(entry),
                "death_reaction" => death_reaction.push(entry),
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
            riposte_skill: Skill::from_entries(riposte_skill),
            move_skill: MoveSkill::from_entry(move_skill.unwrap()),
            tags,
            extra_stack_limit,
            deaths_door: DeathsDoor::from_entry(deaths_door.unwrap()),
            modes: Modes::from_entries(modes),
            incompatible_party_member: Incompatibilities::from_entries(incompatible_party_member),
            death_reaction: DeathReaction::from_entries(death_reaction),
            other,
        })
    }
}

impl Loadable for HeroOverride {
    fn prepare_list(root_path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        let path = root_path.join("heroes");
        if path.exists() {
            collect_paths(&path, |path| Ok(ends_with(path, ".override.darkest")))
        } else {
            Ok(vec![])
        }
    }
    fn load_raw(path: &std::path::Path) -> std::io::Result<Self> {
        let id = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .next()
            .unwrap()
            .to_string();

        let darkest_file = std::fs::read_to_string(path)?;
        let (darkest_file, rest) = darkest_parser().easy_parse(darkest_file.as_str()).unwrap();
        debug_assert_eq!(rest, "");

        // OK, now let's get these parts out...
        let mut resistances = None;
        let mut weapons = vec![];
        let mut armours = vec![];
        let mut skills = vec![];
        let mut riposte_skill = vec![];
        let mut move_skill = None;
        let mut tags = vec![];
        let mut extra_stack_limit = vec![];
        let mut deaths_door = None;
        let mut modes = vec![];
        let mut incompatible_party_member = vec![];
        let mut death_reaction = vec![];
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
                "combat_move_skill" => {
                    let existing = move_skill.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "tag" => tags.extend(entry.get("id").cloned().unwrap()),
                "extra_stack_limit" => extra_stack_limit.extend(entry.get("id").cloned().unwrap()),
                "deaths_door" => {
                    let existing = deaths_door.replace(entry);
                    debug_assert!(existing.is_none());
                }
                "mode" => modes.push(entry),
                "incompatible_party_member" => incompatible_party_member.push(entry),
                "death_reaction" => death_reaction.push(entry),
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
            move_skill: move_skill.map(MoveSkill::from_entry),
            tags,
            extra_stack_limit,
            deaths_door: deaths_door.map(DeathsDoor::from_entry),
            modes: opt_vec(modes).map(Modes::from_entries),
            incompatible_party_member: opt_vec(incompatible_party_member)
                .map(Incompatibilities::from_entries),
            death_reaction: opt_vec(death_reaction).map(DeathReaction::from_entries),
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
struct Skills(BTreeMap<(String, i32), Skill>);

impl Skills {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let mut tmp: HashMap<(String, i32), Vec<DarkestEntry>> = HashMap::new();
        for entry in input {
            let id = entry.get("id").expect("Skill ID field not found")[0].clone();
            let level = entry.get("level").expect("Skill LEVEL field not found")[0]
                .parse()
                .expect("Skill LEVEL field is not a number");
            tmp.entry((id, level)).or_default().push(entry);
        }
        Self(
            tmp.into_iter()
                .map(|(key, value)| (key, Skill::from_entries(value)))
                .collect(),
        )
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "skills");
        let (name, level) = {
            let mut iter = path[1].split('/');
            (
                iter.next().unwrap(),
                iter.next()
                    .unwrap_or_else(|| panic!("Unexpected path in hero data: {:?}, wrong skill ID, expected format <NAME>_<LEVEL>", path))
                    .parse()
                    .unwrap_or_else(|_| panic!("Unexpected path in hero data: {:?}, wrong skill ID, expected format <NAME>_<LEVEL>", path))
            )
        };
        self.0
            .get_mut(&(name.into(), level))
            .unwrap_or_else(|| panic!("Unexpected path in hero data: {:?}, skill not found", path))
            .apply(path, change);
    }
}

impl BTreeMappable for Skills {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        for ((name, level), skill) in &self.0 {
            let map = skill.to_map();
            out.extend_prefixed(&format!("{}/{}", name, level), map);
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Skill {
    effects: Vec<String>,
    other: HashMap<String, String>,
}

impl Skill {
    fn from_entries(mut input: Vec<DarkestEntry>) -> Self {
        let effects = input
            .iter_mut()
            .flat_map(|entry| entry.remove("effect").unwrap_or_default())
            .collect();
        let other: HashMap<_, _> = input
            .into_iter()
            .flat_map(|entry| entry.into_iter())
            .map(|(key, v)| (key, v.join(" ")))
            .collect();
        Self { effects, other }
    }
    fn apply(&mut self, mut path: Vec<String>, change: ItemChange) {
        debug!("Patching skill: path = {:?}, change = {:?}", path, change);
        let key = match path[0].as_str() {
            "skills" => {
                assert!(path.len() == 3);
                path.pop().unwrap()
            }
            "riposte_skill" => {
                assert!(path.len() == 2);
                path.pop().unwrap()
            }
            _ => panic!("Unexpected path in hero info: {:?}", path),
        };
        if path.pop() == Some("effects".to_string()) {
            // they should be patched in other way
            return;
        }
        match change.into_option().map(GameDataValue::unwrap_string) {
            Some(s) => self.other.insert(key, s),
            None => self.other.remove(&key),
        };
    }
}

impl BTreeMappable for Skill {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.extend_prefixed("effects", self.effects.to_map());
        out.extend(
            self.other
                .clone()
                .into_iter()
                .map(|(key, value)| (vec![key], value.into())),
        );
        out
    }
}

#[derive(Clone, Debug)]
struct MoveSkill {
    forward: i32,
    backward: i32,
}
impl MoveSkill {
    fn from_entry(input: DarkestEntry) -> Self {
        let mut dmg = input
            .get("move")
            .expect("Move skill MOVE field not found")
            .iter()
            .map(|s| s.parse().expect("Move skill MOVE field is not a number"));
        Self {
            backward: dmg.next().expect("Move skill MOVE field is empty"),
            forward: dmg
                .next()
                .expect("Move skill MOVE field has only one entry"),
        }
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "move_skill");
        match path[1].as_str() {
            "forward" => self.forward = change.unwrap_set().unwrap_i32(),
            "backward" => self.backward = change.unwrap_set().unwrap_i32(),
            _ => panic!("Unexpected key in hero info patch: {:?}", path),
        };
    }
}

impl BTreeMappable for MoveSkill {
    fn to_map(&self) -> DataMap {
        let mut out = DataMap::new();
        out.insert(vec!["forward".into()], self.forward.into());
        out.insert(vec!["backward".into()], self.backward.into());
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
        debug_assert!(path.len() == 3);
        debug_assert_eq!(path[0], "deaths_door");
        let place = match path[1].as_str() {
            "buffs" => &mut self.buffs,
            "recovery_buffs" => &mut self.recovery_buffs,
            "recovery_heart_attack_buffs" => &mut self.recovery_heart_attack_buffs,
            _ => panic!("Unexpected key in hero info patch: {:?}", path),
        };
        patch_list(place, path, change);
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
struct DeathReaction(Vec<String>);
impl DeathReaction {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        Self(input.into_iter().map(|entry| entry.to_string()).collect())
    }
    fn apply(&mut self, path: Vec<String>, change: ItemChange) {
        debug_assert_eq!(path[0], "death_reaction");
        assert!(path.len() == 2);
        patch_list(&mut self.0, path, change);
    }
}

impl BTreeMappable for DeathReaction {
    fn to_map(&self) -> DataMap {
        self.0.to_set()
    }
}
