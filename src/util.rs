use std::{
    collections::HashMap,
    hash::Hash,
};

use newtype::NewType;

#[derive(Debug, Clone, NewType)]
pub struct HashableHashmap<K: Hash + PartialEq, V>(pub HashMap<K, V>);

impl<K: Eq + Hash, V: Eq> PartialEq for HashableHashmap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        let mut seen_same_keys = 0;
        for k in self.0.keys() {
            if self.0.get(k) != other.get(k) {
                return false;
            }
            seen_same_keys += 1;
        }
        seen_same_keys == other.keys().len()
    }
}
impl<K: Eq + Hash, V: Eq> Eq for HashableHashmap<K, V> {}

impl<K, V> Hash for HashableHashmap<K, V>
where
    K: Hash + PartialEq,
    V: Hash + PartialEq,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (key, value) in self.iter() {
            key.hash(state);
            value.hash(state);
        }
    }
}
#[derive(Debug, Clone, NewType)]
pub struct HashableFloat(pub f32);
impl Hash for HashableFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.to_bits())
    }
}
impl PartialEq for HashableFloat {
    fn eq(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}
impl Eq for HashableFloat {}

#[derive(Debug, Clone, NewType)]
pub struct HashableDouble(pub f64);

impl Hash for HashableDouble {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.to_bits())
    }
}
impl PartialEq for HashableDouble {
    fn eq(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}
impl Eq for HashableDouble {}
