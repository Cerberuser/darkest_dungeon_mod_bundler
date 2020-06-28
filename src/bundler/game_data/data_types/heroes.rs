use super::super::{file_types::DarkestEntry, BTreeMappable, BTreePatchable, Loadable};
use std::{
    collections::{hash_map::Entry, HashMap},
    convert::TryInto,
};

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
}

impl BTreeMappable for HeroInfo {
    fn to_map(&self) -> crate::bundler::diff::DataMap {
        todo!()
    }
}

impl BTreePatchable for HeroInfo {
    fn merge_patches(
        &self,
        patches: impl IntoIterator<Item = crate::bundler::ModFileChange>,
    ) -> (
        crate::bundler::diff::Patch,
        Vec<crate::bundler::ModFileChange>,
    ) {
        todo!()
    }
    fn apply_patch(&mut self, patch: crate::bundler::diff::Patch) -> Result<(), ()> {
        todo!()
    }
}

impl Loadable for HeroInfo {
    fn prepare_list(root_path: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        todo!()
    }
    fn load_raw(path: &std::path::Path) -> std::io::Result<Self> {
        todo!()
    }
}

#[derive(Clone, Debug)]
struct Resistances {
    stun: i32,
    poison: i32,
    bleed: i32,
    disease: i32,
    moving: i32,
    debuff: i32,
    death_blow: i32,
    trap: i32,
}

impl Resistances {
    fn from_entry(input: DarkestEntry) -> Self {
        macro_rules! extract {
            ($($key:literal -> $ident:ident),+) => {
                $(
                    let $ident = input.get($key).unwrap_or_else(|| panic!("Malformed hero information file, no {} resistance found", $key));
                    assert_eq!($ident.len(), 1, "Malformed hero information file: {} resistance have multiple values", $key);
                    let $ident = $ident[0].trim_end_matches('%').parse().unwrap_or_else(|_| panic!("Malformed hero information file, {} resistance is not an integer", $key));
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
        let out: &[_; 5] = out.as_slice().try_into().expect("Should be exactly 5 weapons");
        Self(out.to_owned())
    }
}
impl Weapon {
    fn from_entry(input: DarkestEntry) -> Self {
        let mut out = Self::default();
        out.atk = input.get("atk").expect("Weapon ATK not found").get(0).expect("Weapon ATK field is empty")
            .trim_end_matches('%')
            .parse()
            .expect("Weapon ATK is not a number");
        let mut dmg = input
            .get("dmg")
            .unwrap()
            .into_iter()
            .map(|s| s.parse().unwrap());
        out.dmg = (dmg.next().unwrap(), dmg.next().unwrap());
        out.crit = input.get("crit").unwrap()[0]
            .trim_end_matches('%')
            .parse()
            .unwrap();
        out.spd = input.get("spd").unwrap()[0].parse().unwrap();
        out
    }
}

#[derive(Clone, Debug)]
struct Armours([Armour; 5]);
#[derive(Clone, Debug, Default)]
struct Armour {
    def: f32,
    prot: i32,
    hp: i32,
    spd: i32,
}

impl Armours {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let out: Vec<_> = input.into_iter().map(Armour::from_entry).collect();
        let out: &[_; 5] = out.as_slice().try_into().expect("Should be exactly 5 armours");
        Self(out.to_owned())
    }
}
impl Armour {
    fn from_entry(input: DarkestEntry) -> Self {
        let mut out = Self::default();
        out.def = input.get("def").unwrap()[0]
            .trim_end_matches('%')
            .parse()
            .unwrap();
        out.prot = input.get("prot").unwrap()[0].parse().unwrap();
        out.hp = input.get("hp").unwrap()[0].parse().unwrap();
        out.spd = input.get("spd").unwrap()[0].parse().unwrap();
        out
    }
}

#[derive(Clone, Debug)]
struct Skills(HashMap<(String, i32), Skill>);

impl Skills {
    fn from_entries(input: Vec<DarkestEntry>) -> Self {
        let mut tmp: HashMap<(String, i32), Vec<DarkestEntry>> = HashMap::new();
        for entry in input {
            let id = entry.get("id").unwrap()[0].clone();
            let level = entry.get("level").unwrap()[0].parse().unwrap();
            tmp.entry((id, level)).or_default().push(entry);
        }
        Self(
            tmp.into_iter()
                .map(|(key, value)| (key, Skill::from_entries(value)))
                .collect(),
        )
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
        let mut other: HashMap<_, _> = input
            .into_iter()
            .flat_map(|entry| entry.into_iter())
            .map(|(key, v)| (key, v.join(" ")))
            .collect();
        other.remove("effects");
        Self { effects, other }
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
            .unwrap()
            .into_iter()
            .map(|s| s.parse().unwrap());
        Self {
            backward: dmg.next().unwrap(),
            forward: dmg.next().unwrap(),
        }
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
}
