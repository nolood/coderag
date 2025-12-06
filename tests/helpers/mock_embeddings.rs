use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct MockEmbedder {
    dimension: usize,
}

impl MockEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    pub fn text_to_vector(&self, text: &str) -> Vec<f32> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Generate deterministic vector from hash
        let mut vector = Vec::with_capacity(self.dimension);
        let mut seed = hash;

        for _ in 0..self.dimension {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let value = ((seed / 65536) % 1000) as f32 / 1000.0;
            vector.push(value);
        }

        // Normalize vector
        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for v in vector.iter_mut() {
                *v /= magnitude;
            }
        }

        vector
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub async fn embed(&self, texts: &[String]) -> Vec<Vec<f32>> {
        texts
            .iter()
            .map(|t| self.text_to_vector(t))
            .collect()
    }
}