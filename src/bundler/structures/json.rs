use super::BTreeMappable;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, iter::once};

#[derive(Clone, PartialOrd, PartialEq, Ord, Eq, Debug)]
enum JsonPathPart {
    Index(usize),
    Key(String),
}
impl From<usize> for JsonPathPart {
    fn from(index: usize) -> Self {
        Self::Index(index)
    }
}
impl From<&str> for JsonPathPart {
    fn from(key: &str) -> Self {
        Self::Key(key.into())
    }
}
impl From<&String> for JsonPathPart {
    fn from(key: &String) -> Self {
        Self::Key(key.into())
    }
}
impl From<String> for JsonPathPart {
    fn from(key: String) -> Self {
        Self::Key(key)
    }
}

type JsonPath = Vec<JsonPathPart>;
impl super::MapPath for JsonPath {}

struct JsonFile(Value);

fn flatten(prefix: JsonPath, value: &Value) -> Vec<(JsonPath, &Value)> {
    match value {
        v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
            vec![(prefix, v)]
        }
        Value::Array(arr) => arr
            .iter()
            .enumerate()
            .flat_map(move |(index, value)| {
                flatten(
                    prefix
                        .clone()
                        .into_iter()
                        .chain(once(index.into()))
                        .collect(),
                    value,
                )
            })
            .collect(),
        Value::Object(obj) => obj
            .iter()
            .flat_map(move |(key, value)| {
                flatten(
                    prefix.clone().into_iter().chain(once(key.into())).collect(),
                    value,
                )
            })
            .collect(),
    }
}

fn flatten_mut(prefix: JsonPath, value: &mut Value) -> Vec<(JsonPath, &mut Value)> {
    match value {
        v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
            vec![(prefix, v)]
        }
        Value::Array(arr) => arr
            .iter_mut()
            .enumerate()
            .flat_map(move |(index, value)| {
                flatten_mut(
                    prefix
                        .clone()
                        .into_iter()
                        .chain(once(index.into()))
                        .collect(),
                    value,
                )
            })
            .collect(),
        Value::Object(obj) => obj
            .iter_mut()
            .flat_map(move |(key, value)| {
                flatten_mut(
                    prefix.clone().into_iter().chain(once(key.into())).collect(),
                    value,
                )
            })
            .collect(),
    }
}

fn flatten_owned(prefix: JsonPath, value: Value) -> Vec<(JsonPath, Value)> {
    match value {
        v @ Value::Null | v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
            vec![(prefix, v)]
        }
        Value::Array(arr) => arr
            .into_iter()
            .enumerate()
            .flat_map(move |(index, value)| {
                flatten_owned(
                    prefix
                        .clone()
                        .into_iter()
                        .chain(once(index.into()))
                        .collect(),
                    value,
                )
            })
            .collect(),
        Value::Object(obj) => obj
            .into_iter()
            .flat_map(move |(key, value)| {
                flatten_owned(
                    prefix.clone().into_iter().chain(once(key.into())).collect(),
                    value,
                )
            })
            .collect(),
    }
}

impl BTreeMappable for JsonFile {
    type Key = JsonPath;
    type Value = Value;
    fn map(&self) -> BTreeMap<Self::Key, &Self::Value> {
        flatten(vec![], &self.0).into_iter().collect()
    }
    fn map_mut(&mut self) -> BTreeMap<Self::Key, &mut Self::Value> {
        flatten_mut(vec![], &mut self.0).into_iter().collect()
    }
    fn clone_with(&self, f: impl FnOnce(&mut BTreeMap<Self::Key, Self::Value>)) -> Self {
        let mut map = flatten_owned(vec![], self.0.clone()).into_iter().collect();
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
            let mut insertion_path = String::new();
            let mut inserted_key = None;
            for part in path {
                match part {
                    JsonPathPart::Index(index) => {
                        value = value.as_array_mut().unwrap().remove(0);
                        match dest.get_mut(index) {
                            Some(item) => {
                                dest = item;
                                insertion_path.push_str(&format!("/{}", index));
                            }
                            None => {
                                inserted_key = Some(part);
                                break;
                            }
                        }
                    }
                    JsonPathPart::Key(ref key) => {
                        value = value.as_object_mut().unwrap().remove(key).unwrap();
                        match dest.get_mut(key) {
                            Some(item) => {
                                dest = item;
                                insertion_path.push_str(&format!(
                                    "/{}",
                                    key.replace('~', "~0").replace('/', "~1")
                                ));
                            }
                            None => {
                                inserted_key = Some(part);
                                break;
                            }
                        }
                    }
                }
            }
            match (inserted_key, root.pointer_mut(&insertion_path)) {
                (Some(JsonPathPart::Index(index)), Some(Value::Array(arr))) => {
                    // Due to the BTreeMap ordering guarantees, this index is definitely out of range.
                    debug_assert!(arr.len() <= index);
                    arr.resize_with(index + 1, Default::default);
                    arr[index] = value;
                }
                (Some(JsonPathPart::Key(ref key)), Some(Value::Object(obj))) => {
                    obj.insert(key.clone(), value);
                }
                (key, value) => panic!(
                    "JSON was modified incompatibly: key {:?} is going to be inserted into {:?}",
                    key, value
                ),
            }
        }
        Self(root)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn flatten_and_rebuild() {
        let json = r#"{
            "simple": "string",
            "object": {
                "inner": {
                    "number": 1.2
                },
                "list": [
                    {
                        "inner": true
                    },
                    "in_list"
                ]
            }
        }"#;
        let value: Value = json.parse().unwrap();
        let file = JsonFile(value.clone());
        let file = file.clone_with(|_| {});
        assert_eq!(value, file.0);
    }

    #[test]
    fn modify() {
        let source = r#"{"root": {"string": "old", "number": 1}}"#;
        let target = r#"{"root": {"string": "new", "bool": true}}"#;
        let source_value = source.parse().unwrap();
        let target_value: Value = target.parse().unwrap();
        let file = JsonFile(source_value);
        let file = file.clone_with(|map| {
            map.remove(&vec!["root".into(), "number".into()]);
            map.insert(vec!["root".into(), "bool".into()], true.into());
            map.entry(vec!["root".into(), "string".into()]).and_modify(|e| *e = "new".into());
        });
        assert_eq!(file.0, target_value);
    }
}
