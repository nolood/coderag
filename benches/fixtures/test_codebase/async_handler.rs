//! Async functions and handlers using Tokio

use tokio::sync::{RwLock, Mutex};
use tokio::time::{sleep, Duration};
use std::sync::Arc;
use std::collections::HashMap;

/// Async data processor
pub struct AsyncProcessor {
    cache: Arc<RwLock<HashMap<String, String>>>,
    counter: Arc<Mutex<u64>>,
}

impl AsyncProcessor {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Process data asynchronously with caching
    pub async fn process(&self, key: String, data: String) -> Result<String, String> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        // Simulate async processing
        sleep(Duration::from_millis(100)).await;

        // Process the data
        let result = self.transform_data(data).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(key, result.clone());
        }

        // Increment counter
        {
            let mut counter = self.counter.lock().await;
            *counter += 1;
        }

        Ok(result)
    }

    async fn transform_data(&self, data: String) -> Result<String, String> {
        // Simulate async transformation
        tokio::task::yield_now().await;
        Ok(data.to_uppercase())
    }

    pub async fn get_stats(&self) -> (usize, u64) {
        let cache_size = self.cache.read().await.len();
        let counter = *self.counter.lock().await;
        (cache_size, counter)
    }
}

/// Async batch processor using tokio::spawn
pub async fn batch_process(items: Vec<String>) -> Vec<Result<String, String>> {
    let tasks: Vec<_> = items
        .into_iter()
        .map(|item| {
            tokio::spawn(async move {
                // Simulate async work
                sleep(Duration::from_millis(50)).await;
                process_item(item).await
            })
        })
        .collect();

    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(format!("Task failed: {}", e))),
        }
    }

    results
}

async fn process_item(item: String) -> Result<String, String> {
    if item.is_empty() {
        return Err("Empty item".to_string());
    }
    Ok(format!("Processed: {}", item))
}

/// Graceful shutdown handler
pub async fn run_with_shutdown(
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                println!("Shutdown signal received");
                break;
            }
            _ = async_work() => {
                // Continue working
            }
        }
    }

    // Cleanup
    cleanup().await?;
    Ok(())
}

async fn async_work() {
    sleep(Duration::from_secs(1)).await;
    println!("Working...");
}

async fn cleanup() -> Result<(), Box<dyn std::error::Error>> {
    println!("Cleaning up...");
    sleep(Duration::from_millis(100)).await;
    Ok(())
}