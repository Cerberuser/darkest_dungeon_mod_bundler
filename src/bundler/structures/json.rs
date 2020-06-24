use super::BTreeMappable;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    iter::{empty, once},
};

#[derive(Clone, PartialOrd, PartialEq, Ord, Eq)]
enum JsonPathPart {
    Index(usize),
    Key(String),
}
impl From<usize> for JsonPathPart {
    fn from(index: usize) -> Self {
        Self::Index(index)
    }
}
impl From<&String> for JsonPathPart {
    fn from(key: &String) -> Self {
        Self::Key(key.into())
    }
}

type JsonPath = Vec<JsonPathPart>;
impl super::MapPath for JsonPath {}

struct JsonFile(Value);

fn flatten(
    prefix: impl Iterator<Item = JsonPathPart> + Clone,
    value: &Value,
) -> Vec<(JsonPath, &Value)> {
    match value {
        v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
            vec![(prefix.collect(), v)]
        }
        Value::Array(arr) => arr
            .iter()
            .enumerate()
            .flat_map(move |(index, value)| {
                flatten(prefix.clone().chain(once(index.into())), value)
            })
            .collect(),
        Value::Object(obj) => obj
            .iter()
            .flat_map(move |(key, value)| flatten(prefix.clone().chain(once(key.into())), value))
            .collect(),
    }
}

fn flatten_mut(
    prefix: impl Iterator<Item = JsonPathPart> + Clone,
    value: &mut Value,
) -> Vec<(JsonPath, &mut Value)> {
    match value {
        v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
            vec![(prefix.collect(), v)]
        }
        Value::Array(arr) => arr
            .iter_mut()
            .enumerate()
            .flat_map(move |(index, value)| {
                flatten_mut(prefix.clone().chain(once(index.into())), value)
            })
            .collect(),
        Value::Object(obj) => obj
            .iter_mut()
            .flat_map(move |(key, value)| {
                flatten_mut(prefix.clone().chain(once(key.into())), value)
            })
            .collect(),
    }
}

fn flatten_owned(
    prefix: impl Iterator<Item = JsonPathPart> + Clone,
    value: Value,
) -> Vec<(JsonPath, Value)> {
    match value {
        v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
            vec![(prefix.collect(), v)]
        }
        Value::Array(arr) => arr
            .into_iter()
            .enumerate()
            .flat_map(move |(index, value)| {
                flatten_owned(prefix.clone().chain(once(index.into())), value)
            })
            .collect(),
        Value::Object(obj) => obj
            .into_iter()
            .flat_map(move |(key, value)| {
                flatten_owned(prefix.clone().chain(once((&key).into())), value)
            })
            .collect(),
    }
}

impl BTreeMappable for JsonFile {
    type Key = JsonPath;
    type Value = Value;
    fn map(&self) -> BTreeMap<Self::Key, &Self::Value> {
        flatten(empty(), &self.0).into_iter().collect()
    }
    fn map_mut(&mut self) -> BTreeMap<Self::Key, &mut Self::Value> {
        flatten_mut(empty(), &mut self.0).into_iter().collect()
    }
    fn clone_with(&self, f: impl FnOnce(&mut BTreeMap<Self::Key, Self::Value>)) -> Self {
        let mut map = flatten_owned(empty(), self.0.clone()).into_iter().collect();
        f(&mut map);

        debug_assert!(!map.is_empty());
        let mut root = match self.0 {
            Value::Array(_) => Value::Array(vec![]),
            Value::Object(_) => Value::Object(Map::new()),
            _ => panic!("We're not supposed to have JSON files with the lonely primitive, are we?"),
        };

        for (path, mut value) in map {
            for part in path.clone().into_iter().rev() {
                match part {
                    JsonPathPart::Index(_) => value = Value::Array(vec![value]),
                    JsonPathPart::Key(key) => value = Value::Object(once((key, value)).collect()),
                }
            }
            let mut dest = &mut root;
            let mut src = &mut value;
            for part in path {
                match (dest, part, src) {
                    (Value::Array(arr), JsonPathPart::Index(index), Value::Array(new)) => {
                        let existing = arr.get_mut(index);
                        if let Some(next) = existing {
                            dest = next;
                            src = &mut new[0];
                            continue;
                        }
                        debug_assert!(arr.len() == index);
                        debug_assert!(new.len() == 1);
                        arr.push(new.pop().unwrap());
                        break;
                    }
                    (Value::Object(obj), JsonPathPart::Key(ref key), Value::Object(new)) => todo!(),
                    _ => panic!("JSON was modified incompatibly"),
                }
            }
        };
        Self(root)
    }
}
