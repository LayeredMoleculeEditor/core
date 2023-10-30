pub mod core;

pub mod utils {
    use std::{
        borrow::Borrow,
        collections::{HashMap, HashSet},
        fmt::Debug,
        hash::Hash,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UniqueValueMap<K: Hash + Eq + Clone, V: Hash + Eq + Clone> {
        key_map: HashMap<K, V>,
        validate: HashSet<V>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum InsertResult<K, V> {
        Updated(V),
        Duplicated(K),
        Created,
    }

    impl<K: Clone + Hash + Eq, V: Clone + Hash + Eq> UniqueValueMap<K, V> {
        pub fn new() -> Self {
            Self {
                key_map: HashMap::new(),
                validate: HashSet::new(),
            }
        }

        pub fn data(&self) -> &HashMap<K, V> {
            &self.key_map
        }

        pub fn insert(&mut self, k: K, v: V) -> InsertResult<K, V> {
            if self.validate.contains(&v) {
                self.key_map
                    .iter()
                    .find_map(|(key, value)| if value == &v { Some(key) } else { None })
                    .and_then(|key| Some(InsertResult::Duplicated(key.clone())))
                    .unwrap()
            } else {
                self.validate.insert(v.clone());
                let result = self
                    .key_map
                    .insert(k, v)
                    .and_then(|v| Some(InsertResult::Updated(v)))
                    .unwrap_or(InsertResult::Created);
                if let InsertResult::Updated(v) = &result {
                    self.validate.remove(v);
                }
                result
            }
        }

        pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, k: &Q) -> Option<V>
        where
            K: Borrow<Q>,
        {
            let removed = self.key_map.remove(k);
            if let Some(value) = &removed {
                self.validate.remove(value);
            }
            removed
        }
    }

    #[test]
    fn uniq_val_map() {
        let mut map: UniqueValueMap<String, usize> = UniqueValueMap::new();
        let x = map.insert("center".to_string(), 1);
        let y = map.insert("center".to_string(), 2);
        let z = map.insert("entry".to_string(), 2);
        assert_eq!(x, InsertResult::Created);
        assert_eq!(y, InsertResult::Updated(1));
        assert_eq!(z, InsertResult::Duplicated("center".to_string()));
        assert_eq!(
            map,
            UniqueValueMap {
                key_map: HashMap::from([("center".to_string(), 2)]),
                validate: HashSet::from([2])
            }
        )
    }
}
