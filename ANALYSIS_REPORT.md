# Investigation: Анализ качества проекта CodeRAG

**Дата:** 2025-12-07
**Версия:** 1.0

---

## Research Goal

Комплексный анализ проекта на предмет архитектурных проблем, качества кода, багов, безопасности и производительности.

## Codebase Context

- **Тип проекта:** Rust semantic code search engine с MCP сервером
- **Размер:** 97 Rust файлов
- **Стек:** Tokio, Rayon, LanceDB, Tree-sitter, tantivy (BM25)
- **Ключевые модули:** embeddings, indexing, storage, search, mcp, watcher

---

## Investigation Summary

### Общая оценка

Проект **хорошо структурирован** с современными Rust паттернами (async/await, traits, proper error handling). Однако обнаружен ряд серьёзных проблем, требующих внимания.

### Результаты тестов

- **5 тестов упало:**
  - 3 теста C/C++ парсинга (namespace, struct, pointer functions)
  - 1 тест путей macOS (symlink resolution)
  - 1 flaky timing test

---

## Критические проблемы (Critical)

| # | Проблема | Файл | Описание |
|---|----------|------|----------|
| 1 | **SQL Injection** | `src/storage/lancedb.rs:467` | Пути с кавычками ломают query: `format!("file_path = '{}'", path)` |
| 2 | **Race Condition в Registry** | `src/registry/global.rs:42-88` | Нет file locking между процессами, TOCTOU уязвимость |
| 3 | **Query Injection в BM25** | `src/search/bm25.rs:160` | Пути не экранируются в Tantivy query |

### Детали критических проблем

#### 1. SQL/Query Injection в LanceDB Delete

**Файл:** `src/storage/lancedb.rs:467`

```rust
table.delete(&format!("file_path = '{}'", path_str))
```

**Проблема:** Файловые пути, содержащие одинарные кавычки, сломают запрос. Вредоносные имена файлов потенциально могут инжектировать произвольные SQL/filter выражения.

**Воздействие:** Может вызвать повреждение данных или неожиданные удаления. Уязвимость безопасности, если обрабатываются пользовательские пути.

#### 2. Registry Race Condition

**Файл:** `src/registry/global.rs:42-88`

**Проблема:** Методы `load()` и `save()` используют atomic rename, но имеют TOCTOU (Time-of-check to time-of-use) race condition между несколькими процессами.

**Воздействие:** Параллельные процессы (например, несколько MCP серверов, параллельные CLI вызовы) могут потерять обновления registry. Один процесс может перезаписать изменения, сделанные другим.

#### 3. BM25 Query Injection

**Файл:** `src/search/bm25.rs:160`

```rust
.parse_query(&format!("\"{}\"", file_path))
```

**Проблема:** Файловые пути, содержащие кавычки, не экранируются должным образом, что может сломать парсинг Tantivy query или инжектировать синтаксис запроса.

**Воздействие:** Может вызвать ошибки поиска или неожиданное поведение запросов.

---

## Высокий приоритет (High)

| # | Проблема | Файл | Описание |
|---|----------|------|----------|
| 4 | **Nested Runtime Deadlock** | `src/embeddings/fastembed_provider.rs:196-218` | Создание runtime внутри async контекста, `expect()` в spawned thread |
| 5 | **Poisoned Lock Recovery** | `src/search/bm25.rs:295-313` | Восстановление из poisoned lock скрывает corruption |
| 6 | **Mutex в Rayon без timeout** | `src/indexing/parallel.rs:279` | `lock().unwrap()` в parallel коде может заблокировать все threads |
| 7 | **Unbounded Error Collection** | `src/indexing/parallel.rs:96` | До 1000 ошибок × FileError может занять много памяти |

### Детали проблем высокого приоритета

#### 4. Blocking Code in Async Context - Potential Deadlock

**Файл:** `src/embeddings/fastembed_provider.rs:196-218`

```rust
if let Ok(handle) = tokio::runtime::Handle::try_current() {
    let provider = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new()
            .expect("Failed to create runtime");
        rt.block_on(...)
    }).join()...
}
```

**Проблема:** Вложенное создание runtime подвержено ошибкам и может привести к deadlock в определённых async контекстах. `expect()` внутри spawned thread вызовет невосстановимый crash, если создание runtime не удастся.

#### 5. Lock Poisoning Recovery May Hide Corruption

**Файл:** `src/search/bm25.rs:295-313`

```rust
self.index.write().unwrap_or_else(|poisoned| {
    poisoned.into_inner()
})
```

**Проблема:** Код восстанавливается из poisoned locks, принимая "потенциальную несогласованность". Это может привести к тихому повреждению данных.

**Воздействие:** Если thread паникует, удерживая BM25 lock, последующие операции могут работать с повреждённым состоянием индекса без какого-либо предупреждения.

#### 6. Mutex Lock in Parallel Code Without Timeout

**Файл:** `src/indexing/parallel.rs:279`

```rust
let mut chunker = ast_chunker.lock().unwrap();
```

**Проблема:** Внутри `spawn_blocking` с Rayon `par_iter` захватывается mutex, который может блокировать threads бесконечно. Нет timeout или альтернативы try_lock.

**Воздействие:** Если операции AstChunker медленные или в deadlock, все Rayon threads могут быть заблокированы, остановив весь процесс индексации.

#### 7. Unbounded Error Collection

**Файл:** `src/indexing/parallel.rs:96`

```rust
let error_collector = ErrorCollector::new(1000);
```

**Проблема:** Хотя есть лимит в 1000 ошибок, этот лимит - на одну операцию индексации. Каждый `FileError` содержит path, message и stage - это может накопить значительную память при множестве ошибок.

---

## Средний приоритет (Medium)

| # | Проблема | Файл | Описание |
|---|----------|------|----------|
| 8 | MAX_QUERY_ROWS fallback на 10M | `src/storage/lancedb.rs:146-158` | При ошибке count_rows используется fallback 10M |
| 9 | Staleness check не реализован | `src/auto_index/service.rs:213-218` | TODO в коде, mtime проверка не работает |
| 10 | Log cleanup не реализован | `src/config.rs:405-407` | Опция конфигурации есть, но не реализована |
| 11 | Нет валидации конфигурации | `src/config.rs` | chunk_size=0 вызовет division by zero |
| 12 | Нет circuit breaker для providers | `src/embeddings/registry.rs` | Повторные вызовы к failing provider |
| 13 | Unbounded memory в Accumulator | `src/watcher/accumulator.rs:36-38` | Нет лимита на накопленные изменения |
| 14 | Parser pool size = 1 | `src/indexing/parallel.rs:84` | Размер пула захардкожен |

### Детали проблем среднего приоритета

#### 8. MAX_QUERY_ROWS Fallback

**Файл:** `src/storage/lancedb.rs:19-21,146-158`

```rust
const MAX_QUERY_ROWS: usize = 10_000_000;

async fn get_row_count_or_max(table: &Table) -> usize {
    match table.count_rows(None).await {
        Ok(count) => count,
        Err(e) => {
            warn!(...);
            MAX_QUERY_ROWS  // Fallback to 10M
        }
    }
}
```

**Проблема:** Если `count_rows()` завершается с ошибкой, fallback на 10M строк предполагает весь dataset. Если фактическое количество выше, данные могут быть тихо обрезаны.

#### 9. Staleness Check Not Implemented

**Файл:** `src/auto_index/service.rs:213-218`

```rust
// TODO: Implement mtime-based staleness check
// For now, if index exists, we assume it's up to date.
debug!("Index exists, assuming up to date (mtime check not yet implemented)");
Ok(false)
```

**Проблема:** Политика `OnMissingOrStale` фактически не проверяет staleness.

**Воздействие:** Изменённые файлы могут не переиндексироваться автоматически.

#### 10. Log File Cleanup Not Implemented

**Файл:** `src/config.rs:405-407`

```rust
/// TODO: Not yet implemented - log file cleanup is not currently performed
#[allow(dead_code)]
pub max_files: usize,
```

**Проблема:** Опция конфигурации существует, но не реализована.

**Воздействие:** Лог-файлы могут расти неограниченно, потенциально заполняя дисковое пространство.

#### 11. No Validation of Configuration Values

**Файл:** `src/config.rs`

**Проблема:** Значения конфигурации как `chunk_size`, `batch_size`, `vector_weight`, `bm25_weight` не валидируются.

**Конкретные проблемы:**
- `chunk_size = 0` может вызвать division by zero
- `vector_weight + bm25_weight` может превышать 1.0
- Отрицательные значения могут вызвать неожиданное поведение

#### 12. No Circuit Breaker for Provider Fallback

**Файл:** `src/embeddings/registry.rs`

**Проблема:** Когда providers fail, есть retry логика, но нет circuit breaker паттерна.

**Воздействие:** Повторные вызовы к failing provider могут вызвать cascade failures и плохой user experience.

#### 13. Watcher Accumulator Unbounded Memory

**Файл:** `src/watcher/accumulator.rs:36-38`

```rust
pub struct ChangeAccumulator {
    changes: HashMap<PathBuf, FileChange>,
    ...
}
```

**Проблема:** Нет максимального лимита размера на накопленные изменения. Во время массовых файловых операций (например, git checkout с тысячами файлов), память может расти неограниченно.

**Воздействие:** Потенциальный OOM во время больших операций файловой системы.

#### 14. Parser Pool Size Not Configurable

**Файл:** `src/indexing/parallel.rs:84`

**Проблема:** Экземпляры AstChunker создаются один раз и разделяются через Mutex. Размер пула эффективно равен 1.

**Воздействие:** Сериализация операций AST parsing несмотря на параллельную обработку файлов.

---

## Низкий приоритет (Low)

| # | Проблема | Файл |
|---|----------|------|
| 15 | Clippy warnings: derivable Default | Множество файлов |
| 16 | Unused `handle` variable | `src/embeddings/fastembed_provider.rs:197` |
| 17 | Delete всегда возвращает 1 | `src/watcher/handler.rs:232-244` |
| 18 | Visibility extraction не реализован | `src/indexing/parallel.rs:302` и другие |
| 19 | Flaky timing-based тесты | `src/watcher/batch_detector.rs:114-127` |
| 20 | Dead test helpers | `tests/helpers/test_harness.rs` |

---

## Что сделано хорошо

| Аспект | Оценка | Комментарий |
|--------|--------|-------------|
| **Без unsafe кода** | Отлично | Весь код безопасный Rust |
| **Error handling** | Хорошо | anyhow + context propagation |
| **Модульность** | Отлично | Чёткое разделение ответственности |
| **Типизация** | Отлично | newtypes, enums, traits |
| **Документация** | Хорошо | doc comments на public API |
| **Async consistency** | Хорошо | Правильное использование async/await |
| **Multi-language AST** | Отлично | 7 языков поддержано |
| **Security checks** | Хорошо | Path traversal protection в MCP |

---

## Solution Approaches Compared

### Для SQL Injection (Critical #1)

| Подход | За | Против | Сложность |
|--------|-----|--------|-----------|
| **Параметризованные запросы** | Безопасно, стандартно | Нужна поддержка от LanceDB | Low |
| **Proper escaping** | Быстро реализовать | Не 100% надёжно | Low |
| **Whitelist validation** | Дополнительная защита | Может отклонить валидные пути | Low |

### Для Registry Race Condition (Critical #2)

| Подход | За | Против | Сложность |
|--------|-----|--------|-----------|
| **File locking (flock)** | Стандартный механизм | Не работает на всех FS | Medium |
| **Advisory lock + retry** | Надёжнее | Сложнее реализовать | Medium |
| **SQLite вместо JSON** | ACID гарантии | Зависимость, миграция | High |

### Для Nested Runtime (High #4)

| Подход | За | Против | Сложность |
|--------|-----|--------|-----------|
| **spawn_blocking только** | Проще | Меньше контроля | Low |
| **Dedicated runtime thread** | Изоляция | Overhead | Medium |
| **Sync-only provider** | Убрать async | Блокирует caller | Low |

---

## Recommendation

### Немедленно исправить (Critical)

1. **SQL Injection**: Использовать escaping или параметризованные запросы
   ```rust
   // Вместо format!
   let escaped = path_str.replace("'", "''");
   table.delete(&format!("file_path = '{}'", escaped))
   ```

2. **Registry locking**: Добавить `fs2::FileExt::lock_exclusive()`
   ```rust
   use fs2::FileExt;
   let file = File::open(&path)?;
   file.lock_exclusive()?;
   // ... операции
   file.unlock()?;
   ```

3. **BM25 escaping**: Экранировать специальные символы Tantivy

### Краткосрочно (High)

4. Упростить runtime nesting в FastEmbed - использовать только `spawn_blocking`
5. Заменить poisoned lock recovery на proper error propagation
6. Использовать `try_lock()` или timeout в parallel code

### Среднесрочно

7. Добавить валидацию конфигурации при загрузке
8. Реализовать staleness check (mtime comparison)
9. Ограничить размер accumulator с overflow handling
10. Исправить falling тесты C/C++

### Долгосрочно

11. Извлекать visibility из AST
12. Реализовать log file cleanup
13. Рассмотреть parser pool scaling
14. Добавить circuit breaker для providers

---

## Relevant Files

| Файл | Проблемы |
|------|----------|
| `src/storage/lancedb.rs:467` | SQL injection |
| `src/registry/global.rs:42-88` | Race condition |
| `src/search/bm25.rs:160,295-313` | Query injection + poisoned lock |
| `src/embeddings/fastembed_provider.rs:196-218` | Runtime nesting |
| `src/indexing/parallel.rs:84,279,96` | Parser pool + mutex + error collection |
| `src/auto_index/service.rs:213-218` | TODO staleness |
| `src/config.rs` | Validation gaps, log cleanup TODO |
| `src/watcher/accumulator.rs:36-38` | Unbounded memory |
| `src/embeddings/registry.rs` | No circuit breaker |

---

## Summary Metrics

| Категория | Количество |
|-----------|-----------|
| **Critical** | 3 |
| **High** | 4 |
| **Medium** | 7 |
| **Low** | 6 |
| **Тесты упало** | 5 |
| **Всего проблем** | 20 |

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        CodeRAG Architecture                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │   CLI    │    │   MCP    │    │   Web    │    │  Watcher │  │
│  │ Commands │    │  Server  │    │   UI     │    │  Service │  │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘    └────┬─────┘  │
│       │               │               │               │         │
│       └───────────────┴───────────────┴───────────────┘         │
│                           │                                      │
│  ┌────────────────────────┴────────────────────────────┐        │
│  │                   Core Services                      │        │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │        │
│  │  │  Indexer    │  │   Search    │  │   Symbol    │  │        │
│  │  │  (Parallel) │  │  (Hybrid)   │  │   Index     │  │        │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  │        │
│  └─────────┼────────────────┼────────────────┼─────────┘        │
│            │                │                │                   │
│  ┌─────────┴────────────────┴────────────────┴─────────┐        │
│  │                   Data Layer                         │        │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │        │
│  │  │  LanceDB    │  │    BM25     │  │  Registry   │  │        │
│  │  │  (Vectors)  │  │  (tantivy)  │  │   (JSON)    │  │        │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │        │
│  └──────────────────────────────────────────────────────┘        │
│                                                                  │
│  ┌──────────────────────────────────────────────────────┐        │
│  │                   Providers                          │        │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │        │
│  │  │  FastEmbed  │  │   OpenAI    │  │ Tree-sitter │  │        │
│  │  │  (Local)    │  │   (API)     │  │  (Parsers)  │  │        │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │        │
│  └──────────────────────────────────────────────────────┘        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Next Steps

- `/fix-issue-rag` - Исправить критические уязвимости (SQL injection, race condition)
- `/build-feature-rag` - Реализовать недостающую функциональность (staleness check, config validation)
- `/review` - Провести полный code review

---

*Отчёт сгенерирован автоматически на основе анализа кодовой базы*
