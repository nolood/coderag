use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A thread-safe cache implementation
pub struct Cache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    data: Arc<Mutex<HashMap<K, V>>>,
    max_size: usize,
}

impl<K, V> Cache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    /// Create a new cache with the specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            max_size,
        }
    }

    /// Insert a key-value pair into the cache
    pub fn insert(&self, key: K, value: V) {
        let mut data = self.data.lock().unwrap();
        if data.len() >= self.max_size && !data.contains_key(&key) {
            // Simple eviction: remove the first item
            if let Some(first_key) = data.keys().next().cloned() {
                data.remove(&first_key);
            }
        }
        data.insert(key, value);
    }

    /// Get a value from the cache
    pub fn get(&self, key: &K) -> Option<V> {
        let data = self.data.lock().unwrap();
        data.get(key).cloned()
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut data = self.data.lock().unwrap();
        data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic_operations() {
        let cache: Cache<String, i32> = Cache::new(10);

        cache.insert("key1".to_string(), 100);
        assert_eq!(cache.get(&"key1".to_string()), Some(100));

        cache.insert("key2".to_string(), 200);
        assert_eq!(cache.get(&"key2".to_string()), Some(200));

        cache.clear();
        assert_eq!(cache.get(&"key1".to_string()), None);
    }
}