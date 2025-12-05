# CodeRAG

Локальный MCP-сервер для семантического поиска по кодовой базе.

## Суть

Индексируешь проект один раз → потом любая LLM (Claude, DeepSeek, локальная) может мгновенно находить релевантные куски кода через MCP, не загружая весь проект в контекст.

## Проблема которую решает

- Контекст LLM ограничен (даже 200k токенов — это не весь проект)
- Закидывать всё в контекст — дорого и медленно
- Модель "теряет" информацию в середине большого контекста

## Как работает

```
1. ИНДЕКСАЦИЯ
   
   Первичная: все файлы → чанки → эмбеддинги → LanceDB
   
   Инкрементальная: только изменённые файлы
   - Сохраняем mtime каждого файла
   - При повторном запуске сравниваем
   - Переиндексируем только то, что изменилось
   
   Watch mode: следим за изменениями в реальном времени
   - fsnotify отслеживает файловую систему
   - Автоматически переиндексирует при сохранении
   
2. ПОИСК (каждый запрос)
   
   Запрос → эмбеддинг → поиск ближайших векторов → топ-N чанков
   
3. MCP
   
   LLM вызывает tool search("как обработать платёж") → 
   получает релевантные куски кода
```

## Стек

- **Rust** — один бинарник, без зависимостей
- **fastembed-rs** — локальные эмбеддинги, никаких API
- **LanceDB** — embedded векторная база (как SQLite, но для векторов)
- **tree-sitter** — умное разбиение кода по AST (функции, классы)
- **ignore** — обход файлов с поддержкой .gitignore
- **rmcp** — MCP сервер на Rust

## Структура

```
coderag/
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI: init, index, serve
│   ├── indexer/
│   │   ├── mod.rs
│   │   ├── walker.rs     # обход файлов
│   │   └── chunker.rs    # разбиение на чанки (tree-sitter)
│   ├── embeddings/
│   │   ├── mod.rs
│   │   └── fastembed.rs  # генерация эмбеддингов
│   ├── storage/
│   │   ├── mod.rs
│   │   └── lancedb.rs    # работа с LanceDB
│   ├── search/
│   │   ├── mod.rs
│   │   └── vector.rs     # векторный поиск
│   └── mcp/
│       ├── mod.rs
│       └── server.rs     # MCP сервер
└── config/
    └── default.toml      # дефолтные настройки
```

## CLI

```bash
# Инициализация в проекте
coderag init

# Индексация (первичная или инкрементальная)
coderag index

# Запуск MCP сервера
coderag serve

# Поиск из командной строки (для отладки)
coderag search "обработка платежей"
```

## Конфигурация

```toml
# .coderag/config.toml

[indexer]
# Паттерны для игнорирования (в дополнение к .gitignore)
ignore = ["*.min.js", "vendor/", "dist/"]

# Расширения файлов для индексации
extensions = ["rs", "py", "ts", "js", "go", "java"]

# Максимальный размер чанка (в токенах)
chunk_size = 512

[embeddings]
# Модель для эмбеддингов
model = "nomic-embed-text-v1.5"

# Размер батча при индексации
batch_size = 100

[search]
# Количество результатов по умолчанию
default_limit = 10

[mcp]
# Транспорт: stdio или http
transport = "stdio"

# Порт для http
port = 3000
```

## MCP Tools

### search

Семантический поиск по кодовой базе.

```json
{
  "name": "search",
  "description": "Найти релевантные куски кода по запросу",
  "parameters": {
    "query": "string — поисковый запрос",
    "limit": "number — количество результатов (default: 10)"
  }
}
```

### list_files

Список проиндексированных файлов.

```json
{
  "name": "list_files",
  "description": "Получить список файлов в индексе",
  "parameters": {
    "pattern": "string — glob-паттерн (optional)"
  }
}
```

### get_file

Получить содержимое файла целиком.

```json
{
  "name": "get_file",
  "description": "Получить полное содержимое файла",
  "parameters": {
    "path": "string — путь к файлу"
  }
}
```

## Этапы разработки

### MVP (v0.1)

- [ ] CLI скелет (clap)
- [ ] Обход файлов с .gitignore
- [ ] Простой чанкинг (по строкам)
- [ ] fastembed интеграция
- [ ] LanceDB хранилище
- [ ] Базовый поиск
- [ ] MCP сервер с одним tool (search)

### v0.2

- [ ] Tree-sitter чанкинг (по AST)
- [ ] Инкрементальная переиндексация
- [ ] Watch mode (автоматическая переиндексация при изменениях)
- [ ] Больше MCP tools (list_files, get_file)

### v0.3

- [ ] Гибридный поиск (векторы + ключевые слова)
- [ ] Поддержка нескольких проектов
- [ ] Web UI для отладки
- [ ] Метрики и статистика

## Зависимости (Cargo.toml)

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# Файлы
ignore = "0.4"
walkdir = "2"

# Эмбеддинги
fastembed = "4"

# Хранилище
lancedb = "0.15"
arrow-array = "53"
arrow-schema = "53"

# Парсинг кода
tree-sitter = "0.24"
tree-sitter-rust = "0.23"
tree-sitter-python = "0.23"
tree-sitter-typescript = "0.23"

# MCP
rmcp = "0.1"

# Утилиты
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

## Примеры использования

### С Claude Desktop

```json
// claude_desktop_config.json
{
  "mcpServers": {
    "coderag": {
      "command": "coderag",
      "args": ["serve"],
      "cwd": "/path/to/your/project"
    }
  }
}
```

### С Claude CLI

```bash
claude --mcp "coderag serve"
```

### Standalone (для отладки)

```bash
$ coderag search "валидация email"

Found 3 results:

1. src/validators/email.rs:15-45 (score: 0.89)
   pub fn validate_email(input: &str) -> Result<Email, ValidationError> {
       ...
   }

2. src/api/handlers/user.rs:102-115 (score: 0.76)
   // Validate email before creating user
   let email = validate_email(&req.email)?;
   ...

3. tests/validators_test.rs:50-80 (score: 0.71)
   #[test]
   fn test_email_validation() {
       ...
   }
```
