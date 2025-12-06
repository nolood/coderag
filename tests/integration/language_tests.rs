use anyhow::Result;
use coderag::indexer::Chunker;

#[tokio::test]
async fn test_rust_chunking() -> Result<()> {
    let rust_code = r#"
use std::collections::HashMap;

/// A complex generic struct
pub struct Container<T>
where
    T: Clone + Default,
{
    data: Vec<T>,
    index: HashMap<String, usize>,
}

impl<T> Container<T>
where
    T: Clone + Default,
{
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, value: T) {
        let index = self.data.len();
        self.data.push(value);
        self.index.insert(key, index);
    }
}

#[derive(Debug)]
enum Result<T> {
    Success(T),
    Error(String),
}

fn main() {
    let mut container: Container<i32> = Container::new();
    container.insert("key".to_string(), 42);
}
"#;

    let chunker = Chunker::new(512);
    let chunks = chunker.chunk_file(&std::path::Path::new("test.rs"), rust_code);

    assert!(!chunks.is_empty(), "Rust code should produce chunks");

    // Verify language detection
    assert_eq!(chunks[0].language.as_deref(), Some("rust"));

    // Verify important constructs are captured
    let all_content = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all_content.contains("impl<T>"), "Should capture impl block");
    assert!(all_content.contains("struct Container"), "Should capture struct");
    assert!(all_content.contains("fn main"), "Should capture main function");

    Ok(())
}

#[tokio::test]
async fn test_python_chunking() -> Result<()> {
    let python_code = r#"
import asyncio
from typing import List, Optional

class DataProcessor:
    """Process data asynchronously"""

    def __init__(self, batch_size: int = 100):
        self.batch_size = batch_size
        self._cache = {}

    async def process_batch(self, items: List[str]) -> List[dict]:
        """Process a batch of items"""
        results = []
        for item in items:
            if item in self._cache:
                results.append(self._cache[item])
            else:
                result = await self._process_single(item)
                self._cache[item] = result
                results.append(result)
        return results

    async def _process_single(self, item: str) -> dict:
        await asyncio.sleep(0.1)
        return {"processed": item, "timestamp": asyncio.get_event_loop().time()}

def decorator_example(func):
    """Example decorator"""
    def wrapper(*args, **kwargs):
        print(f"Calling {func.__name__}")
        return func(*args, **kwargs)
    return wrapper

@decorator_example
def greet(name: str) -> str:
    return f"Hello, {name}!"

if __name__ == "__main__":
    processor = DataProcessor()
    items = ["item1", "item2", "item3"]
    asyncio.run(processor.process_batch(items))
"#;

    let chunker = Chunker::new(512);
    let chunks = chunker.chunk_file(&std::path::Path::new("test.py"), python_code);

    assert!(!chunks.is_empty(), "Python code should produce chunks");

    // Verify language detection
    assert_eq!(chunks[0].language.as_deref(), Some("python"));

    // Verify important constructs are captured
    let all_content = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all_content.contains("class DataProcessor"), "Should capture class");
    assert!(all_content.contains("async def"), "Should capture async functions");
    assert!(all_content.contains("@decorator_example"), "Should capture decorator");

    Ok(())
}

#[tokio::test]
async fn test_typescript_chunking() -> Result<()> {
    let typescript_code = r#"
import React, { useState, useEffect } from 'react';

interface User {
    id: number;
    name: string;
    email: string;
}

interface Props {
    initialUsers: User[];
    onUserSelect: (user: User) => void;
}

const UserList: React.FC<Props> = ({ initialUsers, onUserSelect }) => {
    const [users, setUsers] = useState<User[]>(initialUsers);
    const [filter, setFilter] = useState('');

    useEffect(() => {
        // Filter users when filter changes
        const filtered = initialUsers.filter(user =>
            user.name.toLowerCase().includes(filter.toLowerCase())
        );
        setUsers(filtered);
    }, [filter, initialUsers]);

    return (
        <div className="user-list">
            <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter users..."
            />
            <ul>
                {users.map(user => (
                    <li key={user.id} onClick={() => onUserSelect(user)}>
                        {user.name} - {user.email}
                    </li>
                ))}
            </ul>
        </div>
    );
};

export default UserList;

// Helper function
export function validateEmail(email: string): boolean {
    const re = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    return re.test(email);
}
"#;

    let chunker = Chunker::new(512);
    let chunks = chunker.chunk_file(&std::path::Path::new("test.tsx"), typescript_code);

    assert!(!chunks.is_empty(), "TypeScript code should produce chunks");

    // Verify language detection (TypeScript and TSX both map to "typescript")
    assert_eq!(chunks[0].language.as_deref(), Some("typescript"));

    // Verify important constructs are captured
    let all_content = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all_content.contains("interface User"), "Should capture interfaces");
    assert!(all_content.contains("React.FC"), "Should capture React components");
    assert!(all_content.contains("useState"), "Should capture hooks");

    Ok(())
}

#[tokio::test]
async fn test_go_chunking() -> Result<()> {
    let go_code = r#"
package main

import (
    "fmt"
    "sync"
)

// Worker represents a concurrent worker
type Worker struct {
    ID int
    Jobs chan Job
    wg *sync.WaitGroup
}

// Job represents a unit of work
type Job struct {
    ID int
    Data string
}

// NewWorker creates a new worker
func NewWorker(id int, wg *sync.WaitGroup) *Worker {
    return &Worker{
        ID: id,
        Jobs: make(chan Job, 100),
        wg: wg,
    }
}

// Start begins processing jobs
func (w *Worker) Start() {
    go func() {
        defer w.wg.Done()
        for job := range w.Jobs {
            w.processJob(job)
        }
    }()
}

func (w *Worker) processJob(job Job) {
    fmt.Printf("Worker %d processing job %d\n", w.ID, job.ID)
}

func main() {
    var wg sync.WaitGroup

    workers := make([]*Worker, 5)
    for i := 0; i < 5; i++ {
        wg.Add(1)
        workers[i] = NewWorker(i, &wg)
        workers[i].Start()
    }

    wg.Wait()
}
"#;

    let chunker = Chunker::new(512);
    let chunks = chunker.chunk_file(&std::path::Path::new("test.go"), go_code);

    assert!(!chunks.is_empty(), "Go code should produce chunks");

    // Verify language detection
    assert_eq!(chunks[0].language.as_deref(), Some("go"));

    // Verify important constructs are captured
    let all_content = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all_content.contains("type Worker struct"), "Should capture structs");
    assert!(all_content.contains("func NewWorker"), "Should capture functions");
    assert!(all_content.contains("func (w *Worker)"), "Should capture methods");

    Ok(())
}

#[tokio::test]
async fn test_java_chunking() -> Result<()> {
    let java_code = r#"
package com.example;

import java.util.*;
import java.util.stream.Collectors;

public class DataService {
    private final Map<String, User> userCache;
    private final Database database;

    public DataService(Database database) {
        this.database = database;
        this.userCache = new HashMap<>();
    }

    public Optional<User> findUser(String id) {
        // Check cache first
        if (userCache.containsKey(id)) {
            return Optional.of(userCache.get(id));
        }

        // Fetch from database
        return database.findById(id)
            .map(user -> {
                userCache.put(id, user);
                return user;
            });
    }

    public List<User> findUsersByAge(int minAge, int maxAge) {
        return database.findAll().stream()
            .filter(user -> user.getAge() >= minAge && user.getAge() <= maxAge)
            .sorted(Comparator.comparing(User::getName))
            .collect(Collectors.toList());
    }

    private static class User {
        private String id;
        private String name;
        private int age;

        // Getters and setters
        public String getId() { return id; }
        public String getName() { return name; }
        public int getAge() { return age; }
    }
}
"#;

    let chunker = Chunker::new(512);
    let chunks = chunker.chunk_file(&std::path::Path::new("test.java"), java_code);

    assert!(!chunks.is_empty(), "Java code should produce chunks");

    // Verify language detection
    assert_eq!(chunks[0].language.as_deref(), Some("java"));

    // Verify important constructs are captured
    let all_content = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all_content.contains("public class DataService"), "Should capture class");
    assert!(all_content.contains("private static class User"), "Should capture inner class");
    assert!(all_content.contains("stream()"), "Should capture lambda/stream operations");

    Ok(())
}

#[tokio::test]
async fn test_chunking_edge_cases() -> Result<()> {
    let chunker = Chunker::new(512);

    // Test empty file
    let chunks = chunker.chunk_file(&std::path::Path::new("empty.rs"), "");
    assert!(chunks.is_empty(), "Empty file should produce no chunks");

    // Test file with only comments
    let comment_only = r#"
// This is a comment
// Another comment
/* Block comment
   spanning multiple lines */
// More comments
"#;
    let chunks = chunker.chunk_file(&std::path::Path::new("comments.rs"), comment_only);
    assert!(!chunks.is_empty(), "Comments should still produce chunks");

    // Test file with Unicode
    let unicode_code = r#"
fn 你好() {
    let emoji = "🦀";
    let japanese = "こんにちは";
    println!("Unicode test: {} {} {}", emoji, japanese, "мир");
}

// 中文注释
fn test_unicode() {
    你好();
}
"#;
    let chunks = chunker.chunk_file(&std::path::Path::new("unicode.rs"), unicode_code);
    assert!(!chunks.is_empty(), "Unicode should be handled");
    assert!(chunks[0].content.contains("🦀"), "Should preserve emojis");
    assert!(chunks[0].content.contains("你好"), "Should preserve Chinese characters");

    // Test very small file
    let tiny = "fn tiny() {}";
    let chunks = chunker.chunk_file(&std::path::Path::new("tiny.rs"), tiny);
    assert_eq!(chunks.len(), 1, "Small file should produce single chunk");
    assert_eq!(chunks[0].content.trim(), tiny);

    Ok(())
}

#[tokio::test]
async fn test_chunking_line_boundaries() -> Result<()> {
    // Test that chunking respects line boundaries properly
    let code = r#"
fn function_1() {
    println!("Line 1");
    println!("Line 2");
    println!("Line 3");
}

fn function_2() {
    println!("Line 1");
    println!("Line 2");
    println!("Line 3");
}

fn function_3() {
    println!("Line 1");
    println!("Line 2");
    println!("Line 3");
}
"#;

    let chunker = Chunker::new(100); // Small chunks to force splitting
    let chunks = chunker.chunk_file(&std::path::Path::new("boundaries.rs"), code);

    // Verify line numbers are consistent
    for chunk in &chunks {
        assert!(chunk.start_line <= chunk.end_line, "Start line should be <= end line");
        assert!(chunk.start_line > 0, "Line numbers should be 1-indexed");
    }

    // Verify chunks don't overlap incorrectly
    for i in 1..chunks.len() {
        let prev_end = chunks[i-1].end_line;
        let curr_start = chunks[i].start_line;
        // Allow for overlap but ensure progression
        assert!(curr_start <= prev_end + 1,
            "Chunks should not have gaps (prev end: {}, curr start: {})",
            prev_end, curr_start);
    }

    Ok(())
}