use crate::types::{FlatKey, FlatValue};
use std::collections::HashMap;

pub trait StateDB {
    fn get_state(&self, key: &FlatKey) -> Option<&FlatValue>;
    fn set_state(&mut self, key: FlatKey, val: FlatValue);
}

pub struct MapState {
    state: HashMap<FlatKey, FlatValue>,
}

impl MapState {
    pub fn new() -> Self {
        MapState {
            state: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.state.len()
    }
}

impl StateDB for MapState {
    fn get_state(&self, key: &FlatKey) -> Option<&FlatValue> {
        self.state.get(key)
    }

    fn set_state(&mut self, key: FlatKey, val: FlatValue) {
        self.state.insert(key, val);
    }
}
