# CodeRAG MCP Agent Guide

## Overview

CodeRAG - семантический поисковик по коду на Rust. Предоставляет MCP сервер с 6 инструментами для LLM.

**Технологии:**
- Embeddings: FastEmbed (nomic-embed-text-v1.5, 768 размерность)
- Векторное хранилище: LanceDB
- Полнотекстовый поиск: Tantivy (BM25)
- AST-парсинг: Tree-sitter (Rust, Python, TypeScript, Go, Java, C/C++)

## Запуск MCP сервера

```bash
# Stdio транспорт (для Claude Desktop)
coderag serve

# HTTP/SSE транспорт
coderag serve --http --port 3000

# Без автоиндексации
coderag serve --no-auto-index
```

**Автоиндексация:** Сервер автоматически индексирует проект при первом запуске если индекс отсутствует.

---

## MCP Tools

### 1. `search` - Семантический поиск кода

**Описание:** Поиск релевантных фрагментов кода по естественному языку.

**Параметры:**
| Параметр | Тип | Обязательный | По умолчанию | Описание |
|----------|-----|--------------|--------------|----------|
| `query` | string | да | - | Поисковый запрос на естественном языке |
| `limit` | number | нет | 10 | Максимум результатов (1-100) |

**Пример запроса:**
```json
{
  "query": "функция для генерации embeddings",
  "limit": 5
}
```

**Формат ответа:**
```
## Search Results for: "функция для генерации embeddings"

### Result 1 (87.5% match)
**File:** src/embeddings/fastembed_provider.rs
**Lines:** 45-89

```rust
pub async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    // ... код ...
}
```

**File Header (first 50 lines):**
```rust
// Первые 50 строк файла для контекста
```
```

**Когда использовать:**
- Поиск реализации функционала по описанию
- Поиск примеров использования API
- Поиск паттернов кода

---

### 2. `list_files` - Список индексированных файлов

**Описание:** Просмотр всех файлов в индексе с опциональной фильтрацией.

**Параметры:**
| Параметр | Тип | Обязательный | По умолчанию | Описание |
|----------|-----|--------------|--------------|----------|
| `pattern` | string | нет | null | Glob-паттерн (e.g., `*.rs`, `src/**/*.ts`) |

**Примеры:**
```json
// Все файлы
{}

// Только Rust файлы
{"pattern": "*.rs"}

// Файлы в src/commands/
{"pattern": "src/commands/**/*.rs"}
```

**Формат ответа:**
```
## Indexed Files

Found 42 files matching pattern "*.rs":

- src/main.rs
- src/lib.rs
- src/config.rs
- src/cli/mod.rs
...
```

**Когда использовать:**
- Изучение структуры проекта
- Поиск файлов определенного типа
- Проверка что файл проиндексирован

---

### 3. `get_file` - Получение содержимого файла

**Описание:** Чтение полного содержимого файла по пути.

**Параметры:**
| Параметр | Тип | Обязательный | Описание |
|----------|-----|--------------|----------|
| `path` | string | да | Путь относительно корня проекта |

**Пример:**
```json
{"path": "src/config.rs"}
```

**Формат ответа:**
```
## File: src/config.rs

```rust
use serde::{Deserialize, Serialize};
// ... полное содержимое файла ...
```
```

**Безопасность:** Путь валидируется - нельзя выйти за пределы корня проекта.

**Когда использовать:**
- Детальное изучение файла после `search`
- Чтение конфигурации
- Анализ полного контекста

---

### 4. `find_symbol` - Поиск символов по имени

**Описание:** Поиск определений функций, классов, структур по имени.

**Параметры:**
| Параметр | Тип | Обязательный | По умолчанию | Описание |
|----------|-----|--------------|--------------|----------|
| `name` | string | да | - | Имя символа |
| `kind` | string | нет | null | Тип: function, class, struct, enum, interface, method, variable |
| `search_mode` | string | нет | "prefix" | Режим: exact, prefix, fuzzy |

**Примеры:**
```json
// Поиск всех функций начинающихся с "embed"
{"name": "embed", "kind": "function"}

// Точный поиск структуры
{"name": "Config", "kind": "struct", "search_mode": "exact"}

// Нечеткий поиск
{"name": "embding", "search_mode": "fuzzy"}
```

**Формат ответа:**
```
## Symbol Search Results for: "embed"

### Symbol 1: embed (function)
**File:** src/embeddings/fastembed_provider.rs:45
**Lines:** 45-89
**Visibility:** public
**Signature:** `pub async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>`

### Symbol 2: embed_query (function)
**File:** src/embeddings/mod.rs:67
...
```

**Когда использовать:**
- Поиск определения функции/класса
- Навигация по коду
- Изучение API

---

### 5. `list_symbols` - Список символов

**Описание:** Список всех символов в файле или соответствующих критериям.

**Параметры:**
| Параметр | Тип | Обязательный | Описание |
|----------|-----|--------------|----------|
| `file_path` | string | нет | Путь к файлу (относительный) |
| `kind_filter` | string | нет | Фильтр по типу символа |

**Примеры:**
```json
// Все символы в файле
{"file_path": "src/config.rs"}

// Только функции в файле
{"file_path": "src/config.rs", "kind_filter": "function"}

// Все структуры в проекте
{"kind_filter": "struct"}
```

**Формат ответа:**
```
## Symbols in src/config.rs

### Structs (3)
- Config (lines 15-45)
- IndexerConfig (lines 47-62)
- SearchConfig (lines 64-75)

### Functions (5)
- load_config (lines 80-95)
- save_config (lines 97-110)
...

### Impl blocks (2)
- impl Config (lines 120-180)
...
```

**Когда использовать:**
- Изучение структуры файла
- Поиск всех функций/классов
- Обзор API модуля

---

### 6. `find_references` - Поиск ссылок на символ

**Описание:** Поиск всех мест использования символа (текстовый поиск).

**Параметры:**
| Параметр | Тип | Обязательный | По умолчанию | Описание |
|----------|-----|--------------|--------------|----------|
| `symbol_name` | string | да | - | Имя символа |
| `max_results` | number | нет | 50 | Максимум результатов |

**Пример:**
```json
{"symbol_name": "EmbeddingGenerator", "max_results": 20}
```

**Формат ответа:**
```
## References to: EmbeddingGenerator

Found 15 references:

### src/embeddings/mod.rs
- Line 12: `pub struct EmbeddingGenerator {`
- Line 45: `impl EmbeddingGenerator {`

### src/search/vector.rs
- Line 8: `use crate::embeddings::EmbeddingGenerator;`
- Line 23: `embedder: Arc<EmbeddingGenerator>,`
...
```

**Ограничения:** Это текстовый поиск, не semantic analysis. Может найти ложные совпадения.

**Когда использовать:**
- Анализ зависимостей
- Рефакторинг
- Понимание использования API

---

## Рекомендуемые стратегии использования

### Стратегия 1: Изучение кодовой базы

```
1. list_files -> получить структуру
2. search("main entry point") -> найти точку входа
3. get_file(main.rs) -> изучить главный файл
4. list_symbols(file_path="src/lib.rs") -> изучить публичное API
```

### Стратегия 2: Поиск реализации

```
1. search("функционал X") -> найти релевантные фрагменты
2. find_symbol(name="function_name") -> найти определение
3. get_file(path) -> получить полный контекст
4. find_references(symbol_name) -> найти использования
```

### Стратегия 3: Анализ зависимостей

```
1. find_symbol(name="ClassName", kind="struct") -> найти структуру
2. list_symbols(file_path, kind_filter="method") -> методы класса
3. find_references("ClassName") -> где используется
```

---

## Поддерживаемые языки

| Язык | Расширения | AST парсинг |
|------|------------|-------------|
| Rust | .rs | ✅ |
| Python | .py | ✅ |
| TypeScript | .ts, .tsx | ✅ |
| JavaScript | .js, .jsx | ✅ |
| Go | .go | ✅ |
| Java | .java | ✅ |
| C | .c, .h | ✅ |
| C++ | .cpp, .cc, .cxx, .hpp, .hxx | ✅ |

---

## Структура данных поиска

**SearchResult содержит:**
```typescript
interface SearchResult {
  content: string;        // Содержимое чанка
  file_path: string;      // Путь к файлу
  start_line: number;     // Начальная строка
  end_line: number;       // Конечная строка
  score: number;          // Релевантность (0.0-1.0)
  file_header?: string;   // Первые 50 строк файла
  semantic_kind?: string; // function|class|struct|etc
  symbol_name?: string;   // Имя функции/класса
  signature?: string;     // Сигнатура
}
```

---

## Конфигурация проекта

Файл `.coderag/config.toml`:

```toml
[indexer]
extensions = ["rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp", "h", "hpp"]
ignore_patterns = ["node_modules", "target", ".git", "dist", "build"]
chunk_size = 512
chunker_strategy = "ast"  # или "line"

[search]
mode = "hybrid"     # vector|bm25|hybrid
vector_weight = 0.7
bm25_weight = 0.3
default_limit = 10

[embeddings]
model = "nomic-embed-text-v1.5"
batch_size = 32
```

---

## Примеры промптов для агента

### Для поиска кода:
```
"Найди функцию, которая генерирует embeddings для текста"
→ search(query="функция генерации embeddings для текста")
```

### Для изучения структуры:
```
"Какие файлы есть в директории src/commands?"
→ list_files(pattern="src/commands/**/*.rs")
```

### Для анализа функции:
```
"Покажи реализацию функции search"
→ find_symbol(name="search", kind="function")
→ get_file(найденный путь)
```

### Для рефакторинга:
```
"Где используется структура Config?"
→ find_references(symbol_name="Config")
```

---

## Ограничения и особенности

1. **Индексация:** Первый запуск может занять время на больших проектах (1-5 минут)
2. **Размер чанков:** 512 токенов по умолчанию, большие функции могут разбиваться
3. **find_references:** Текстовый поиск, не семантический анализ
4. **Обновление индекса:** Требует `coderag index --force` или `coderag watch`
5. **Память:** ~50-100MB RAM на 10k файлов

---

## Интеграция с Claude Desktop

Добавить в `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "coderag": {
      "command": "/path/to/coderag",
      "args": ["serve"],
      "cwd": "/path/to/your/project"
    }
  }
}
```

---

## Пример MCP клиента на Python

```python
import subprocess
import json

def call_mcp_tool(tool_name: str, params: dict) -> str:
    request = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": params
        }
    }

    proc = subprocess.Popen(
        ["coderag", "serve"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        cwd="/path/to/project"
    )

    proc.stdin.write(json.dumps(request).encode() + b"\n")
    proc.stdin.flush()

    response = proc.stdout.readline()
    return json.loads(response)

# Пример использования
results = call_mcp_tool("search", {"query": "error handling", "limit": 5})
```
