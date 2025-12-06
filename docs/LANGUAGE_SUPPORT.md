# Language Support Documentation

CodeRAG provides comprehensive language support through Tree-sitter parsers for accurate AST-based code chunking and symbol extraction.

## Supported Languages

| Language | Extensions | AST Chunking | Symbol Extraction | Test Coverage |
|----------|------------|--------------|-------------------|---------------|
| **Rust** | .rs | ✅ Full | ✅ Full | ✅ Comprehensive |
| **Python** | .py | ✅ Full | ✅ Full | ✅ Comprehensive |
| **TypeScript** | .ts, .tsx | ✅ Full | ✅ Full | ✅ Comprehensive |
| **JavaScript** | .js, .jsx | ✅ Full | ✅ Full | ✅ Comprehensive |
| **Go** | .go | ✅ Full | ✅ Full | ✅ Comprehensive |
| **Java** | .java | ✅ Full | ✅ Full | ✅ Comprehensive |
| **C** | .c, .h | ✅ Full | ✅ Full | ✅ Comprehensive |
| **C++** | .cpp, .cc, .cxx, .hpp | ✅ Full | ✅ Full | ✅ Comprehensive |

## Language-Specific Features

### Rust
```rust
// Supported constructs for chunking:
impl MyStruct {            // Implementation blocks
    fn method(&self) {}    // Methods
}

trait MyTrait {}           // Traits
struct MyStruct {}         // Structs
enum MyEnum {}            // Enums
mod my_module {}          // Modules

// Symbol extraction:
pub fn process_data() {}   // Functions
const MAX_SIZE: usize = 100; // Constants
type Result<T> = std::result::Result<T, Error>; // Type aliases
```

**Chunking Strategy:**
- Functions and methods are individual chunks
- Implementation blocks are kept together if small
- Modules create hierarchical chunks
- Comments are preserved with their associated code

### Python
```python
# Supported constructs for chunking:
class MyClass:             # Classes
    def method(self):      # Methods
        pass

def function():           # Functions
    pass

async def async_func():   # Async functions
    pass

# Symbol extraction:
MY_CONSTANT = 100         # Constants
my_variable = "value"     # Module-level variables

@decorator
def decorated_func():     # Decorated functions
    pass
```

**Chunking Strategy:**
- Classes are chunked with their methods
- Large methods are split into separate chunks
- Decorators are preserved with functions
- Docstrings are included in chunks

### TypeScript/JavaScript
```typescript
// Supported constructs for chunking:
class MyClass {           // Classes
    method() {}          // Methods
}

interface MyInterface {} // Interfaces
type MyType = {}        // Type definitions

function myFunction() {} // Functions
const myConst = () => {} // Arrow functions

export default class {} // Default exports
export { Symbol }       // Named exports

// Symbol extraction:
const CONFIG = {}       // Constants
let variable = 10      // Variables
enum Status {}         // Enums
namespace MyNS {}      // Namespaces
```

**Chunking Strategy:**
- React components are treated as single chunks
- Large components are intelligently split
- JSX is properly handled
- Import statements grouped at file start

### Go
```go
// Supported constructs for chunking:
type MyStruct struct {}    // Structs

func (m *MyStruct) Method() {} // Methods

func Function() {}         // Functions

interface MyInterface {}   // Interfaces

package mypackage         // Packages

// Symbol extraction:
const MaxSize = 100       // Constants
var GlobalVar = "value"   // Variables
type MyType = OtherType   // Type aliases
```

**Chunking Strategy:**
- Methods grouped with their receivers
- Interface definitions kept compact
- Package-level declarations grouped
- Test functions identified and chunked

### Java
```java
// Supported constructs for chunking:
public class MyClass {     // Classes
    public void method() {} // Methods
}

interface MyInterface {}   // Interfaces

enum Status {}            // Enums

@Annotation
public void annotated() {} // Annotations

// Symbol extraction:
public static final int MAX = 100; // Constants
private String field;              // Fields
```

**Chunking Strategy:**
- Inner classes handled hierarchically
- Annotations preserved with elements
- JavaDoc comments included
- Generic types properly parsed

### C
```c
// Supported constructs for chunking:
struct my_struct {        // Structures
    int field;
};

void function() {}        // Functions

typedef struct {} my_type; // Typedefs

#define MAX_SIZE 100      // Macros

// Symbol extraction:
int global_var;           // Global variables
static int static_var;    // Static variables
enum status {};          // Enumerations
```

**Chunking Strategy:**
- Header files treated specially
- Include guards recognized
- Macro definitions grouped
- Function declarations vs definitions

### C++
```cpp
// Supported constructs for chunking:
class MyClass {           // Classes
public:
    void method();        // Method declarations
};

namespace MyNamespace {}  // Namespaces

template<typename T>      // Templates
class Template {};

// Symbol extraction:
constexpr int MAX = 100;  // Constants
inline void func() {}     // Inline functions
using MyAlias = Type;     // Type aliases
```

**Chunking Strategy:**
- Template specializations handled
- Namespace scoping preserved
- Virtual functions identified
- STL usage patterns recognized

## Chunking Algorithm Details

### AST-Based Chunking Process

1. **Parse Source Code**
   ```rust
   let tree = parser.parse(&source_code, None)?;
   let root_node = tree.root_node();
   ```

2. **Identify Semantic Boundaries**
   - Function/method boundaries
   - Class/struct definitions
   - Module/namespace blocks
   - Import groups

3. **Apply Size Constraints**
   ```toml
   [indexer]
   chunk_size = 512        # Target size
   min_chunk_tokens = 50   # Minimum size
   max_chunk_tokens = 1500 # Maximum size
   ```

4. **Preserve Context**
   - Include relevant imports
   - Maintain indentation
   - Keep associated comments
   - Preserve decorators/annotations

### Chunking Examples

#### Good Chunk (Semantic Unit)
```python
def process_payment(order: Order) -> PaymentResult:
    """Process payment for an order."""
    validator = PaymentValidator()
    if not validator.validate(order):
        raise ValidationError("Invalid order")

    gateway = PaymentGateway()
    result = gateway.charge(order.total)

    return PaymentResult(
        success=result.success,
        transaction_id=result.id
    )
```

#### Poor Chunk (Arbitrary Split)
```python
def process_payment(order: Order) -> PaymentResult:
    """Process payment for an order."""
    validator = PaymentValidator()
    if not validator.validate(order):
# <-- Split here breaks logic flow
        raise ValidationError("Invalid order")

    gateway = PaymentGateway()
```

## Symbol Extraction

### Symbol Types by Language

| Symbol Type | Rust | Python | TS/JS | Go | Java | C | C++ |
|-------------|------|---------|--------|-----|------|---|-----|
| Functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Classes | ✅ | ✅ | ✅ | ❌ | ✅ | ❌ | ✅ |
| Methods | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Variables | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Constants | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Types | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Interfaces | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Enums | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| Modules | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |

### Symbol Metadata

Each extracted symbol includes:
```json
{
  "name": "processPayment",
  "kind": "function",
  "file_path": "src/payments.ts",
  "line": 45,
  "column": 8,
  "signature": "function processPayment(order: Order): Promise<Result>",
  "visibility": "public",
  "async": true,
  "generic_params": ["T"],
  "decorators": ["@authenticated"],
  "doc_comment": "Process a payment for the given order"
}
```

## Language Detection

### Automatic Detection
CodeRAG automatically detects languages based on:
1. File extensions
2. Shebang lines (`#!/usr/bin/env python`)
3. File content analysis

### Extension Mapping
```toml
[languages]
rust = ["rs"]
python = ["py", "pyw"]
typescript = ["ts", "tsx"]
javascript = ["js", "jsx", "mjs", "cjs"]
go = ["go"]
java = ["java"]
c = ["c", "h"]
cpp = ["cpp", "cc", "cxx", "hpp", "hxx", "h++"]
```

### Override Detection
```bash
# Force language for specific files
coderag index --language python --file script
```

## Fallback Strategies

When AST parsing fails or is unavailable:

### Line-Based Chunking
```toml
[indexer]
chunker_strategy = "line"  # Fallback to line-based
```

Features:
- Works with any text file
- Splits on natural boundaries (empty lines, indentation)
- Preserves context windows
- Language-agnostic

### Heuristic Symbol Detection
For unsupported languages, CodeRAG uses patterns:
- Function: `function`, `def`, `fn`, `func`
- Class: `class`, `struct`, `type`
- Variable: `const`, `let`, `var`, `val`

## Adding Language Support

### Requirements for New Languages

1. **Tree-sitter Grammar**
   ```toml
   [dependencies]
   tree-sitter-newlang = "0.20.0"
   ```

2. **Language Configuration**
   ```rust
   pub struct NewLangConfig {
       extensions: vec!["ext"],
       tree_sitter_language: tree_sitter_newlang::language(),
       chunk_patterns: ChunkPatterns {
           functions: vec!["function_declaration"],
           classes: vec!["class_declaration"],
           // ...
       }
   }
   ```

3. **Test Cases**
   ```rust
   #[test]
   fn test_newlang_chunking() {
       let source = include_str!("fixtures/sample.ext");
       let chunks = chunk_newlang(source);
       assert!(chunks.len() > 0);
   }
   ```

### Contributing a Language

1. Add Tree-sitter dependency
2. Implement language config
3. Add chunking patterns
4. Write comprehensive tests
5. Submit PR with examples

## Performance by Language

| Language | Parse Speed | Chunk Speed | Symbol Extraction |
|----------|-------------|-------------|-------------------|
| Rust | 5ms/file | 10ms/file | 3ms/file |
| Python | 3ms/file | 8ms/file | 2ms/file |
| TypeScript | 8ms/file | 15ms/file | 5ms/file |
| JavaScript | 7ms/file | 12ms/file | 4ms/file |
| Go | 4ms/file | 9ms/file | 3ms/file |
| Java | 6ms/file | 11ms/file | 4ms/file |
| C | 3ms/file | 7ms/file | 2ms/file |
| C++ | 10ms/file | 20ms/file | 7ms/file |

*Average for 1000-line files on modern hardware*

## Best Practices

### 1. Use AST Chunking
Always prefer AST-based chunking for supported languages:
```toml
[indexer]
chunker_strategy = "ast"
```

### 2. Configure Extensions
Include all relevant extensions:
```toml
[indexer]
extensions = ["js", "jsx", "mjs", "cjs", "ts", "tsx"]
```

### 3. Set Appropriate Chunk Sizes
- **Small files**: 256-512 tokens
- **Large files**: 512-1024 tokens
- **Documentation**: 1024-1500 tokens

### 4. Language-Specific Settings
```toml
[languages.python]
include_docstrings = true
include_type_hints = true

[languages.typescript]
include_jsdoc = true
preserve_jsx = true
```

## Troubleshooting

### Issue: Poor Chunking Quality
**Solution**: Verify AST parsing is enabled
```bash
coderag stats --language-details
```

### Issue: Missing Symbols
**Solution**: Update Tree-sitter grammars
```bash
cargo update -p tree-sitter-*
```

### Issue: Slow Parsing
**Solution**: Use parallel indexing
```toml
[indexer]
parallel_threads = null  # Auto-detect
```

### Issue: Unsupported File Types
**Solution**: Use line-based fallback
```toml
[indexer]
chunker_strategy = "line"
unknown_extensions_fallback = true
```