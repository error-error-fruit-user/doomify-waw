//! What a TOML's values are made of, in a form a const can hold: text, lists and maps that borrow
//! what `const_toml!` built into the program, or own what was read while it runs. And every type a
//! TOML is read into, in one place, which is what `const_toml!`'s code names.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};
use std::{borrow::Cow, collections::BTreeMap};

pub use crate::{
    project::{Config, Player, StartItem},
    sounds::{Sound, SoundSet},
    weapon::{Ammo, Burst, Fire, Game, Kind, Look, View, Wall, Weapon},
};

/// Text in a TOML.
pub type Text = Cow<'static, str>;
/// A list in a TOML.
pub type List<T> = Cow<'static, [T]>;

/// A table of a TOML whose keys are its own (roles, sound names, weapon ids): its entries sorted
/// by key, as a `BTreeMap` keeps them, in a slice a const can hold.
#[derive(Debug, Clone, PartialEq)]
pub struct Map<V: Clone + 'static>(pub Cow<'static, [(Text, V)]>);

impl<V: Clone + 'static> Map<V> {
    /// A map of `entries`, which are sorted by key (`const_toml!` writes them so).
    pub const fn new(entries: &'static [(Text, V)]) -> Self {
        Map(Cow::Borrowed(entries))
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0
            .binary_search_by(|(k, _)| k.as_ref().cmp(key))
            .ok()
            .map(|index| &self.0[index].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Text, &V)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn keys(&self) -> impl Iterator<Item = &Text> {
        self.0.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<V: Clone + 'static> Default for Map<V> {
    fn default() -> Self {
        Map::new(&[])
    }
}

impl<V: Clone + 'static> FromIterator<(Text, V)> for Map<V> {
    fn from_iter<I: IntoIterator<Item = (Text, V)>>(entries: I) -> Self {
        let sorted: BTreeMap<Text, V> = entries.into_iter().collect();
        Map(Cow::Owned(sorted.into_iter().collect()))
    }
}

impl<'a, V: Clone + 'static> IntoIterator for &'a Map<V> {
    type Item = (&'a Text, &'a V);
    type IntoIter =
        std::iter::Map<std::slice::Iter<'a, (Text, V)>, fn(&'a (Text, V)) -> (&'a Text, &'a V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

impl<V: Clone + Serialize + 'static> Serialize for Map<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de, V: Clone + Deserialize<'de> + 'static> Deserialize<'de> for Map<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = BTreeMap::<String, V>::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|(k, v)| (Text::Owned(k), v))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_reads_sorted_and_finds_its_keys() {
        let map: Map<u32> = toml::from_str::<BTreeMap<String, Map<u32>>>("[t]\nb = 2\na = 1\n")
            .unwrap()
            .remove("t")
            .unwrap();
        assert_eq!(
            map.keys().map(|k| k.as_ref()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(map.get("b"), Some(&2));
        assert!(!map.contains_key("c"));
        const BUILT: Map<u32> = Map::new(&[(Cow::Borrowed("x"), 7)]);
        assert_eq!(BUILT.get("x"), Some(&7));
    }
}
