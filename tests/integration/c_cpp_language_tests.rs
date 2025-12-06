//! Integration tests for C/C++ language support

use anyhow::Result;
use coderag::indexer::{
    ast_chunker::{ASTChunker, ChunkedFile},
    ChunkerStrategy, IndexerConfig,
};
use std::path::PathBuf;
use tempfile::tempdir;
use tokio;

/// Helper function to create test files
async fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    tokio::fs::write(&path, content).await.unwrap();
    path
}

/// Helper function to chunk a file
async fn chunk_file(path: PathBuf, config: &IndexerConfig) -> Result<ChunkedFile> {
    let chunker = ASTChunker::new(config.clone());
    chunker.chunk_file(&path).await
}

#[tokio::test]
async fn test_c_function_extraction() {
    let dir = tempdir().unwrap();
    let c_code = r#"
#include <stdio.h>

// Calculate factorial
int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

// Main function
int main() {
    printf("Factorial of 5: %d\n", factorial(5));
    return 0;
}
"#;

    let path = create_test_file(dir.path(), "test.c", c_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find 2 functions
    assert_eq!(chunked.chunks.len(), 2);

    // Verify function names
    let names: Vec<_> = chunked.chunks.iter()
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"factorial"));
    assert!(names.contains(&"main"));

    // Verify semantic kinds
    for chunk in &chunked.chunks {
        assert_eq!(chunk.metadata.get("kind").and_then(|v| v.as_str()), Some("function"));
    }
}

#[tokio::test]
async fn test_c_struct_extraction() {
    let dir = tempdir().unwrap();
    let c_code = r#"
// Point structure
struct Point {
    int x;
    int y;
};

// Rectangle structure
struct Rectangle {
    struct Point topLeft;
    struct Point bottomRight;
};

// Calculate area
int calculate_area(struct Rectangle rect) {
    int width = rect.bottomRight.x - rect.topLeft.x;
    int height = rect.bottomRight.y - rect.topLeft.y;
    return width * height;
}
"#;

    let path = create_test_file(dir.path(), "structs.c", c_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find 2 structs and 1 function
    assert_eq!(chunked.chunks.len(), 3);

    // Verify struct names
    let structs: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("struct"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(structs.contains(&"Point"));
    assert!(structs.contains(&"Rectangle"));

    // Verify function exists
    let functions: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("function"))
        .count();
    assert_eq!(functions, 1);
}

#[tokio::test]
async fn test_c_enum_extraction() {
    let dir = tempdir().unwrap();
    let c_code = r#"
// Color enum
enum Color {
    RED = 0,
    GREEN = 1,
    BLUE = 2
};

// Day of week
enum DayOfWeek {
    MONDAY,
    TUESDAY,
    WEDNESDAY,
    THURSDAY,
    FRIDAY,
    SATURDAY,
    SUNDAY
};
"#;

    let path = create_test_file(dir.path(), "enums.c", c_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find 2 enums
    assert_eq!(chunked.chunks.len(), 2);

    // Verify enum names
    let enums: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("enum"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(enums.contains(&"Color"));
    assert!(enums.contains(&"DayOfWeek"));
}

#[tokio::test]
async fn test_c_typedef_extraction() {
    let dir = tempdir().unwrap();
    let c_code = r#"
// Type definitions
typedef unsigned int uint;

typedef struct {
    int x;
    int y;
} Point;

typedef enum {
    FALSE = 0,
    TRUE = 1
} Boolean;
"#;

    let path = create_test_file(dir.path(), "typedefs.c", c_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find type aliases
    let type_aliases: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("type_alias"))
        .count();
    assert!(type_aliases > 0);
}

#[tokio::test]
async fn test_cpp_class_extraction() {
    let dir = tempdir().unwrap();
    let cpp_code = r#"
// Point class
class Point {
private:
    int x, y;

public:
    Point(int x, int y) : x(x), y(y) {}

    int getX() const { return x; }
    int getY() const { return y; }

    double distance() const {
        return sqrt(x*x + y*y);
    }
};

// Circle class
class Circle {
    Point center;
    double radius;

public:
    Circle(Point c, double r) : center(c), radius(r) {}

    double area() const {
        return 3.14159 * radius * radius;
    }
};
"#;

    let path = create_test_file(dir.path(), "classes.cpp", cpp_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find classes and methods
    let classes: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("class"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(classes.contains(&"Point"));
    assert!(classes.contains(&"Circle"));

    // Should also find methods
    let methods: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("method"))
        .count();
    assert!(methods > 0);
}

#[tokio::test]
async fn test_cpp_template_extraction() {
    let dir = tempdir().unwrap();
    let cpp_code = r#"
// Template function
template<typename T>
T max(T a, T b) {
    return (a > b) ? a : b;
}

// Template class
template<typename T>
class Container {
    T* data;
    size_t size;

public:
    Container(size_t s) : size(s) {
        data = new T[size];
    }

    ~Container() {
        delete[] data;
    }

    T& operator[](size_t index) {
        return data[index];
    }
};

// Template specialization
template<>
class Container<bool> {
    // Special implementation for bool
};
"#;

    let path = create_test_file(dir.path(), "templates.cpp", cpp_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find template function and classes
    assert!(chunked.chunks.len() > 0);

    // Verify template function
    let functions: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("function"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(functions.contains(&"max"));

    // Verify template class
    let classes: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("class"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(classes.contains(&"Container"));
}

#[tokio::test]
async fn test_cpp_namespace_extraction() {
    let dir = tempdir().unwrap();
    let cpp_code = r#"
namespace math {
    const double PI = 3.14159;

    double circle_area(double radius) {
        return PI * radius * radius;
    }

    namespace geometry {
        class Shape {
        public:
            virtual double area() = 0;
        };
    }
}

namespace utils {
    template<typename T>
    void swap(T& a, T& b) {
        T temp = a;
        a = b;
        b = temp;
    }
}
"#;

    let path = create_test_file(dir.path(), "namespaces.cpp", cpp_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find namespaces (modules)
    let namespaces: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("module"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(namespaces.contains(&"math"));
    assert!(namespaces.contains(&"utils"));
}

#[tokio::test]
async fn test_cpp_inheritance() {
    let dir = tempdir().unwrap();
    let cpp_code = r#"
// Base class
class Shape {
public:
    virtual double area() = 0;
    virtual double perimeter() = 0;
};

// Derived class
class Rectangle : public Shape {
    double width, height;

public:
    Rectangle(double w, double h) : width(w), height(h) {}

    double area() override {
        return width * height;
    }

    double perimeter() override {
        return 2 * (width + height);
    }
};

// Another derived class
class Circle : public Shape {
    double radius;

public:
    Circle(double r) : radius(r) {}

    double area() override {
        return 3.14159 * radius * radius;
    }

    double perimeter() override {
        return 2 * 3.14159 * radius;
    }
};
"#;

    let path = create_test_file(dir.path(), "inheritance.cpp", cpp_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find base and derived classes
    let classes: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("class"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(classes.contains(&"Shape"));
    assert!(classes.contains(&"Rectangle"));
    assert!(classes.contains(&"Circle"));

    // Should find overridden methods
    let methods: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("method"))
        .count();
    assert!(methods > 0);
}

#[tokio::test]
async fn test_cpp_enum_class() {
    let dir = tempdir().unwrap();
    let cpp_code = r#"
// Traditional enum
enum Color {
    RED,
    GREEN,
    BLUE
};

// Enum class (C++11)
enum class Status : uint8_t {
    OK = 0,
    ERROR = 1,
    PENDING = 2
};

// Another enum class
enum class Direction {
    NORTH,
    SOUTH,
    EAST,
    WEST
};
"#;

    let path = create_test_file(dir.path(), "enum_classes.cpp", cpp_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should find all enums
    let enums: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("enum"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(enums.contains(&"Color"));
    assert!(enums.contains(&"Status"));
    assert!(enums.contains(&"Direction"));
}

#[tokio::test]
async fn test_c_header_file() {
    let dir = tempdir().unwrap();
    let h_code = r#"
#ifndef MYHEADER_H
#define MYHEADER_H

// Function declarations
int add(int a, int b);
int subtract(int a, int b);

// Struct declaration
struct Vector2D {
    float x;
    float y;
};

// Type definitions
typedef struct Vector2D Vec2;

#endif // MYHEADER_H
"#;

    let path = create_test_file(dir.path(), "myheader.h", h_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should extract struct even from header file
    let structs: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("struct"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(structs.contains(&"Vector2D"));
}

#[tokio::test]
async fn test_cpp_header_file() {
    let dir = tempdir().unwrap();
    let hpp_code = r#"
#pragma once

#include <string>

namespace MyLib {
    // Class declaration
    class Manager {
    private:
        std::string name;

    public:
        Manager(const std::string& n);
        ~Manager();

        void process();
        std::string getName() const;
    };

    // Template declaration
    template<typename T>
    class SmartPointer {
        T* ptr;

    public:
        SmartPointer(T* p) : ptr(p) {}
        ~SmartPointer() { delete ptr; }

        T* operator->() { return ptr; }
    };
}
"#;

    let path = create_test_file(dir.path(), "mylib.hpp", hpp_code).await;
    let config = IndexerConfig::default();
    let chunked = chunk_file(path, &config).await.unwrap();

    // Should extract namespace and classes from header
    let modules: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("module"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(modules.contains(&"MyLib"));

    let classes: Vec<_> = chunked.chunks.iter()
        .filter(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("class"))
        .filter_map(|c| c.metadata.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(classes.contains(&"Manager"));
    assert!(classes.contains(&"SmartPointer"));
}

#[tokio::test]
async fn test_mixed_c_cpp_project() {
    let dir = tempdir().unwrap();

    // Create multiple files
    let c_file = r#"
int add(int a, int b) {
    return a + b;
}
"#;

    let cpp_file = r#"
class Calculator {
public:
    int multiply(int a, int b) {
        return a * b;
    }
};
"#;

    let c_path = create_test_file(dir.path(), "math.c", c_file).await;
    let cpp_path = create_test_file(dir.path(), "calculator.cpp", cpp_file).await;

    let config = IndexerConfig::default();

    // Chunk both files
    let c_chunked = chunk_file(c_path, &config).await.unwrap();
    let cpp_chunked = chunk_file(cpp_path, &config).await.unwrap();

    // Verify C file has function
    assert_eq!(c_chunked.chunks.len(), 1);
    assert_eq!(
        c_chunked.chunks[0].metadata.get("kind").and_then(|v| v.as_str()),
        Some("function")
    );

    // Verify C++ file has class and method
    assert!(cpp_chunked.chunks.len() >= 1);
    assert!(cpp_chunked.chunks.iter()
        .any(|c| c.metadata.get("kind").and_then(|v| v.as_str()) == Some("class")));
}