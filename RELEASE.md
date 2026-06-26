# Elitra Lang 1.0.1

Динамический язык программирования с unique-синтаксисом, JIT-компиляцией, опциональной статической типизацией и интерполяцией строк.

Реализован на Rust.

---

## Установка

Требуется Rust: https://rustup.rs

```bash
# Linux / macOS
git clone <repo> && cd elitra
./install/install.sh

# macOS (альтернативно)
./install/install_macos.sh

# Windows
install\install.bat
```

После установки доступен один бинарник `eltr` со всеми командами.

---

## Синтаксис

```elitra
~fib(n)
    when n <= 1
        emit n
    emit fib(n - 1) + fib(n - 2)

say fib(10)        // => 55
shout "fib = {fib(10)}"   // интерполяция строк
```

### Ключевые слова

| Elitra | Аналог |
|--------|--------|
| `cell` | объявление переменной |
| `~` | объявление функции |
| `\` | лямбда |
| `emit` | возврат значения |
| `say` / `shout` | print / println |
| `when` / `else` | if / else |
| `over` | for |
| `pick` | match |
| `dare` / `catch` | try / catch |
| `load` | import |
| `shape` | struct |
| `style` | enum |
| `yes` / `no` | true / false |
| `none` | nil |

---

## Возможности

- **JIT-компиляция** через LLVM (inkwell) — прозрачно, без флагов
- **Опциональная статическая типизация**: `eltr --check-types file.eltr`
- **Форматтер**: `eltr fmt file.eltr`
- **Тестовый раннер**: `eltr test file.eltr` — запускает функции `test_*`
- **LSP-сервер**: `eltr lsp` (stdin/stdout JSON-RPC)
- **Пакетный менеджер**: `eltr init`, `eltr run`, `eltr install`, `eltr build`

### Стандартная библиотека

| Модуль | Функции |
|--------|---------|
| `std/math` | sin, cos, sqrt, pow, log, pi, e, clamp, lerp и др. |
| `std/fs` | read, write, append, exists, create_dir, remove, copy, list_dir |
| `std/os` | get_env, set_env, args, home_dir, current_dir, pid, host_name |
| `std/datetime` | now, timestamp, format (strftime), sleep_ms |
| `std/json` | encode, decode, pretty, validate, encode_file, decode_file |
| `std/str` | trim, upper, lower, contains, replace, split, starts_with, repeat, pad и др. |
| `std/list` | push, pop, sort, reverse, join, slice, unique, flatten, chunk, zip, enumerate |
| `std/net` | http_get, http_post, http_put, http_delete, fetch, tcp_connect, dns_lookup |
| `std/random` | int, float, choice, shuffle, uuid, normal, bytes |

### Встроенные функции

`len`, `str`, `int`, `float`, `bool`, `type`, `split`, `trim`, `upper`, `lower`, `contains`, `replace`, `push`, `pop`, `sort`, `reverse`, `join`, `map`, `filter`, `fold`, `take`, `collect`, `iter`, `abs`, `sin`, `cos`, `sqrt`, `floor`, `ceil`, `round`, `max`, `min`, `pow`, `log`, `exp`, `say`, `shout`, `input`, `read`, `write`, `lines`, `json_encode`, `json_decode`, `json_validate`, `clock`, `exit`, `assert`

---

## Изменения относительно предыдущей версии

### Новый синтаксис (полностью заменён)

Все ключевые слова заменены на уникальные — ни один токен не повторяет Python, Rust, Go, Zig или Elixir.

### Пакетный менеджер

Команды:
- `eltr init [name]` — создать проект с `package.toml` и `src/main.eltr`
- `eltr run` — запустить проект по `package.toml`
- `eltr build` — проверить типы
- `eltr install <pkg>` — добавить зависимость и создать `packages/<pkg>/`

Формат `package.toml`:
```toml
name = "myproject"
version = "0.1.0"
entry = "src/main.eltr"

[dependencies]
foo = "*"
```

### Исправления

- **Double-free в JIT**: при возврате параметра функции (`emit n`) указатель результата алиасил указатель аргумента, что приводило к двойному освобождению памяти. Исправлено — значение клонируется до освобождения.
- **JIT runtime**: `__jit_add` теперь обрабатывает `Value::Int` (раньше работал только с `Value::Float`).
- **Builtins**: `print`/`println` переименованы в `say`/`shout` для консистентности с синтаксисом.

### Удалено

- Мёртвый код из `src/package.rs` (старый парсер `package.toml`) — заменён реализацией в `eltr`.

---

## Известные ограничения

- Нет настоящей асинхронности — `async fn`/`await` синтаксически есть, но исполнение синхронное
- Нет FFI — вызов C/Rust библиотек недоступен
- Нет регексов
- Нет макросов / compile-time метапрограммирования
- Нет traits / интерфейсов
- Нормальные типы ошибок (Result/Option) отсутствуют
- Пакетный менеджер не умеет скачивать пакеты из реестра — только создаёт локальную структуру
- Нет сборщика мусора для циклических ссылок (используется `Rc`)
- Tail-call оптимизация отсутствует
- Нет встроенного профилировщика и дебаггера

---

## Сборка из исходников

```bash
cargo build --release
# Бинарник: target/release/eltr
```

Требуется LLVM 22 (через `inkwell`). Все 67 тестов проходят:

```bash
cargo test --bin eltr
```
