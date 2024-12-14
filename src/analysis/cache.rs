use std::collections::HashMap;
use crate::types::{AnalysisResult, CacheKey};

/// Manages caching of analysis results
pub struct CacheManager {
    cache: HashMap<CacheKey, AnalysisResult>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Store a result in the cache
    pub fn store(&mut self, key: CacheKey, result: AnalysisResult) {
        self.cache.insert(key, result);
    }

    /// Retrieve a result from the cache
    pub fn get(&self, key: &CacheKey) -> Option<&AnalysisResult> {
        self.cache.get(key)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new()
    }
} 