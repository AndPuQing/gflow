use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

/// Type alias for dependency IDs - uses SmallVec to avoid heap allocation for small lists
/// Most jobs have 0-2 dependencies, so inline storage of 2 elements keeps same size as Vec
pub type DependencyIds = smallvec::SmallVec<[u32; 2]>;

/// Type alias for GPU IDs - uses SmallVec to avoid heap allocation for typical GPU counts
/// Most jobs use 1-4 GPUs, so inline storage of 4 elements eliminates heap allocation
pub type GpuIds = smallvec::SmallVec<[u32; 4]>;

/// Job parameters stored as a small Vec of key-value pairs.
///
/// This keeps `Job`'s inline size small (unlike `SmallVec` with large inline tuples)
/// while avoiding `HashMap` hashing overhead for typical small parameter counts.
#[derive(Debug, Clone)]
pub struct Parameters(Vec<(CompactString, CompactString)>);

impl PartialEq for Parameters {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        self.0
            .iter()
            .all(|(k, v)| other.0.iter().any(|(ok, ov)| k == ok && v == ov))
    }
}

impl Eq for Parameters {}

impl Parameters {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn get(&self, key: &str) -> Option<&CompactString> {
        self.0
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
    }

    pub fn insert(&mut self, key: CompactString, value: CompactString) {
        if let Some((_k, v)) = self.0.iter_mut().find(|(k, _)| k == key) {
            *v = value;
        } else {
            self.0.push((key, value));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CompactString, &CompactString)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self::new()
    }
}

impl FromIterator<(CompactString, CompactString)> for Parameters {
    fn from_iter<T: IntoIterator<Item = (CompactString, CompactString)>>(iter: T) -> Self {
        let mut params = Self::new();
        for (k, v) in iter {
            params.insert(k, v);
        }
        params
    }
}

impl<'a> IntoIterator for &'a Parameters {
    type Item = (&'a CompactString, &'a CompactString);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (CompactString, CompactString)>,
        fn(&'a (CompactString, CompactString)) -> (&'a CompactString, &'a CompactString),
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

impl Serialize for Parameters {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Parameters {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map: HashMap<CompactString, CompactString> = HashMap::deserialize(deserializer)?;
        Ok(Self(map.into_iter().collect()))
    }
}

impl crate::utils::ParameterLookup for Parameters {
    fn get_param(&self, key: &str) -> Option<&CompactString> {
        self.get(key)
    }
}
