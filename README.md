# Elitra Lang

Динамический язык программирования с unique-синтаксисом, JIT-компиляцией на LLVM, опциональной статической типизацией и интерполяцией строк.

---

## Что нового в 1.3.0

### Iterator Protocol — цепочечные методы для итераторов

Любой итератор (range, список, строка) поддерживает fluent-цепочки:
```elitra
over x in 1..100 | map(\(n) n * 2) | filter(\(n) n % 3 == 0) | take(5)
    shout(x)
```

Методы: `.map()`, `.filter()`, `.take()`, `.skip()`, `.enumerate()`, `.zip()`, `.chain()`, `.flatten()`, `.collect()`, `.fold()`, `.for_each()`, `.all()`, `.any()`, `.count()`, `.nth()`, `.last()`, `.sum()`, `.min()`, `.max()`

### Runtime Trait Dispatch

Поддержка типажей с динамической диспетчеризацией через vtables:
```elitra
trait Draw
    ~draw(self)

shape Point
    x
    y

impl Draw for Point
    ~draw(self)
        shout "Point"

cell d = as_trait(Point(1, 2), "Draw")
d.draw()  # вызов через vtable
```

### Оператор `|>` (pipe)

Передача значения в функцию или метод слева направо:
```elitra
42 |> str()          # "42"
[1, 2, 3] |> len()   # 3
"hello" |> upper() |> reverse()  # "OLLEH"
items |> map(\(x) x * 2) |> collect()
```

Дешугаринг: `x |> f(y)` → `f(x, y)`, `x |> f` → `f(x)`.

### `throw` — кастомные ошибки

```elitra
~divide(a, b)
    when b == 0
        throw "division by zero"
    emit a / b

dare
    divide(10, 0)
catch e
    shout("Error: {e}")
```

`throw` может принимать любое значение (не только строку), и `catch` получит исходный тип.

### Дефолтные параметры

```elitra
~greet(name, greeting = "Hello")
    shout("{greeting}, {name}!")

greet("World")              # Hello, World!
greet("World", "Hi")        # Hi, World!
```

### Исправления

- Исправлен баг JIT-кэширования: trait dispatch теперь использует уникальное имя функции (`Type_method`) вместо имени метода, что предотвращало вызов неправильного тела функции при нескольких реализациях
- Исправлен баг парсера: `parse_trait` больше не consumes `~` дважды
- `take()` теперь принимает `Value::Int` в дополнение к `Value::Float`
- JIT: `__jit_iter_next` делегирует все типы итераторов интерпретатору, исправляя silent data-loss на сложных цепочках
- GC: `trait_impls` и `std_modules` теперь сканируются сборщиком мусора

### Установка / Удаление

Скрипты установки теперь определяют существующую версию и делают upgrade:
- `install/install.sh` (Linux)
- `install/install_macos.sh` (macOS)
- `install/install.bat` (Windows)
- `install/uninstall.sh` / `install/uninstall_macos.sh` / `install/uninstall.bat`

---

## Установка

Требуется [Rust](https://rustup.rs) и LLVM 22+.

```bash
# Linux / macOS
bash install/install.sh

# macOS (альтернативно)
bash install/install_macos.sh

# Windows
install\install.bat
```

Либо сборка из исходников:

```bash
cargo build --release
# Бинарник: target/release/eltr
```

## Использование

```bash
eltr                    # REPL
eltr script.eltr        # запуск файла
eltr --check-types script.eltr  # с проверкой типов
eltr fmt file.eltr      # форматирование
eltr test file.eltr     # запуск тестов (функции test_*)
eltr lsp                # LSP-сервер (stdin/stdout JSON-RPC)
eltr init [name]        # создать проект
eltr run                # запустить проект
eltr build              # проверить типы
eltr install <pkg>      # добавить зависимость
```

---

## Синтаксис

### Переменные

```elitra
cell x = 5               # динамическая типизация
cell x: int = 5          # с аннотацией типа
x = 10                   # переприсваивание
pub cell pi = 3.14       # публичная переменная (видна при импорте)
```

Типы: `int`, `float`, `string`, `bool`, `list<T>`, `dict<K, V>`, `fn(...) -> T`

### Функции

```elitra
~add(a, b)               # объявление функции
    a + b

~add(a: int, b: int) -> int    # с типами параметров и возврата
    a + b

~greet(name, greeting = "Hello")   # дефолтные параметры
    shout("{greeting}, {name}!")

\(x) x * 2               # анонимная функция (лямбда)

pub ~helper(x)           # публичная функция
    x * 2
```

Тело функции — последнее выражение (неявный возврат) либо `emit`.
Дефолтные параметры вычисляются при каждом вызове (не при определении).

### Управляющие конструкции

```elitra
when x > 0
    say("positive")
else when x == 0
    say("zero")
else
    say("negative")

while i < 10
    say(i)
    i = i + 1

over i in 0..10          # range
    say("{i} ")

over item in items       # список
    shout(item)

over ch in "hello"       # строка
    shout(ch)

do
    shout("body")
while condition

break                    # выход из цикла
continue                 # следующая итерация
```

### Match (сопоставление с образцом)

```elitra
pick x
    1 =>
        "one"
    3 =>
        "three"
    _ =>                 # wildcard
        "other"
```

### Try / Catch

```elitra
dare
    assert(false, "fail!")
catch e
    shout("Error: {e}")

# с throw
dare
    throw MyError("something went wrong")
catch e
    shout(type(e))       # выделенный тип, не строка
```

### Списки и словари

```elitra
cell arr = [1, 2, 3]
cell arr = [1, 2, ]       # trailing comma
arr[0]                    # индексация
arr[0] = 42               # присваивание по индексу

cell d = {"a": 1, "b": 2}
d["a"]                    # доступ по ключу
d["c"] = 3                # вставка
```

### Операторы

| Категория | Операторы |
|-----------|-----------|
| Арифметика | `+`, `-`, `*`, `/`, `%` |
| Составное присваивание | `+=`, `-=`, `*=`, `/=`, `%=` |
| Сравнение | `==`, `!=`, `<`, `<=`, `>`, `>=` |
| Логические | `&&`, `||`, `!` |
| Унарные | `-` (отрицание), `!` (логическое НЕ) |
| Pipe | `|>` (передача значения в функцию) |
| Try | `?` (постфиксный, `expr?` — распаковка `Err`) |
| Диапазон | `..` |
| Доступ | `[]` (индекс), `.` (метод/поле) |

### Интерполяция строк

```elitra
cell name = "world"
cell s = "Hello, {name}!"
cell s = "2 + 3 = {2 + 3}"           # любое выражение
cell s = "\{escaped\}"                # экранирование
```

### Импорты и пакетный менеджер

```elitra
load "math.eltr"                   # выполнить файл (общий скоуп)
load "math.eltr" as math           # как модуль (изолированный скоуп)
math.sin(3.14)                     # вызов через модуль
```

Поиск модулей:
- `./foo`, `../bar`, `dir/baz` — relative к текущему файлу
- `foo` (bare name) — CWD → `~/.eltr/packages/` (ищет `foo.eltr`, `foo/foo.eltr`)
- `std/xxx` — встроенные модули

`package.toml` — метаданные пакета:

```toml
name = "mylib"
version = "0.1.0"
description = "My library"

[dependencies]
foo = "*"
```

### Комментарии

```elitra
// однострочный
/* многострочный */
```

### Структуры, перечисления и классы

```elitra
shape Point                  # структура
    x: int
    y: int

style Color                  # перечисление
    Red
    Green
    Blue
    Rgb(r: int, g: int, b: int)

class Animal                 # класс (поддерживает наследование)
    ~__init__(self, name)
        self.name = name

    ~speak(self)
        shout("...")

class Dog < Animal           # наследование
    ~__init__(self, name)
        super.__init__(name)  # вызов родителя

    ~speak(self)
        shout("Woof!")
```

### Типажи (traits)

```elitra
trait Draw
    ~draw(self)

    # метод с реализацией по умолчанию
    ~describe(self)
        shout("a drawable thing")

shape Circle
    radius

impl Draw for Circle
    ~draw(self)
        shout("Circle(r={self.radius})")
    # describe() — используется из trait

cell c = Circle(10)
c.draw()                     # прямой вызов через impl
cell d = as_trait(Circle(5), "Draw")
d.draw()                     # вызов через vtable (runtime dispatch)
```

### Асинхронность

```elitra
async ~fetch_data(url)
    cell resp = await fetch(url)
    emit resp

async ~main()
    cell result = await fetch_data("https://example.com")
    shout(result)
```

### Truthiness

Значение считается ложным, если:
- `none`
- `no`
- `0`
- `""` (пустая строка)
- `[]` (пустой список)
- `{}` (пустой словарь)

Всё остальное — истинно.

### Ключевые слова

| Elitra | Аналог |
|--------|--------|
| `cell` | объявление переменной |
| `pub` | модификатор видимости |
| `~` | объявление функции |
| `\` | лямбда |
| `emit` | возврат значения |
| `say` / `shout` | print / println |
| `when` / `else` | if / else |
| `while` | цикл с предусловием |
| `do` / `while` | цикл с постусловием |
| `over` / `in` | for |
| `break` | выход из цикла |
| `continue` | следующая итерация |
| `pick` | match |
| `dare` / `catch` | try / catch |
| `throw` | выброс ошибки |
| `load` / `as` | import / alias |
| `shape` | struct |
| `style` | enum |
| `class` / `super` | класс / родитель |
| `trait` / `impl` | типаж / реализация |
| `async` / `await` | асинхронный вызов |
| `yes` / `no` | true / false |
| `none` | nil |
| `pub` | модификатор доступа |

---

## JIT-компиляция

Elitra использует LLVM через `inkwell` для JIT-компиляции горячих путей. Включение:

```bash
eltr --jit script.eltr
```

JIT компилирует функции при первом вызове и кэширует скомпилированный код.
Итераторы, trait dispatch и `throw` при JIT прозрачно делегируются интерпретатору.

---

## Проверка типов

`--check-types` включает статический анализ. Числа выводятся как `float`, но аннотация `int` принимается (`int` совместим с `float`).

| Проверка | Описание |
|----------|----------|
| Типы выражений | соответствие аннотации и значения |
| Return type | функция с `-> T` обязана содержать `emit` |
| Dead code | варнинг после `emit`/`break`/`continue` |
| Неопределённые переменные | обращение к несуществующей переменной |
| Типы аргументов | совместимость при присваивании |

```bash
eltr --check-types script.eltr
```

---

## Форматирование ошибок

Ошибки компиляции и рантайма отображаются с указанием строки и колонки:

```
TypeError at 3:10:
  cell x: int = "hello"
               ^^^^^^^
Type error: expected 'int', got 'string'
```

---

## Сборщик мусора

Elitra использует stop-the-world mark-sweep GC. Сборка запускается автоматически при нехватке памяти.
GC корректно обрабатывает `TraitObject`, замыкания, итераторы, модули и классовые экземпляры.

---

## Встроенные модули (std/)

| Модуль | Функции |
|--------|---------|
| `std/math` | sin, cos, sqrt, pow, log, pi, e, clamp, lerp, deg_to_rad, max, min |
| `std/fs` | read, write, append, exists, is_file, is_dir, create_dir, remove, copy, list_dir |
| `std/os` | name, arch, args, env, get_env, set_env, exit, sleep, current_dir, pid, host_name |
| `std/datetime` | now, year, month, day, hour, minute, second, timestamp, format |
| `std/json` | encode, decode, pretty, validate |
| `std/str` | len, upper, lower, trim, split, contains, replace, reverse, repeat, starts_with, ends_with, pad, bytes, capitalize, title |
| `std/list` | len, push, pop, sort, reverse, join, first, last, slice, unique, flatten, chunk, zip, enumerate, map, filter, reduce |
| `std/net` | http_get, http_post, fetch |
| `std/random` | int, float, choice, shuffle, uuid |

Плоские builtin-функции (`sin()`, `read()`, ...) остаются доступны для обратной совместимости.

---

## Встроенные функции

| Функция | Описание |
|---------|----------|
| `say(...)` | вывод без переноса строки |
| `shout(...)` | вывод с переносом |
| `str(x)` | строка |
| `int(x)` | целое число (floor) |
| `float(x)` | число с плавающей точкой |
| `bool(x)` | логическое значение |
| `type(value)` | имя типа |
| `len(x)` | длина строки/списка/словаря |
| `split(s, sep)` | разделить строку |
| `trim(s)` | обрезать пробелы |
| `upper(s)` / `lower(s)` | регистр |
| `contains(haystack, needle)` | содержит подстроку |
| `replace(s, from, to)` | заменить подстроку |
| `push(list, item)` | добавить элемент |
| `pop(list)` | удалить последний |
| `sort(list)` | отсортировать |
| `reverse(list)` | развернуть |
| `join(list, sep)` | объединить в строку |
| `as_trait(value, trait_name)` | упаковать в trait object |
| `abs(x)` | модуль числа |
| `floor(x)` / `ceil(x)` / `round(x)` | округление |
| `sin(x)` / `cos(x)` / `sqrt(x)` | тригонометрия |
| `max(a, b)` / `min(a, b)` | минимум/максимум |
| `pow(a, b)` / `log(x)` / `exp(x)` | степени и логарифмы |
| `read(path)` | чтение файла |
| `write(path, content)` | запись файла |
| `lines(path)` | чтение строк файла |
| `input()` | чтение строки из stdin |
| `json_encode(value)` | сериализация JSON |
| `json_decode(string)` | десериализация JSON |
| `clock()` | время с эпохи Unix |
| `assert(cond, msg?)` | проверка условия |
| `exit(code?)` | завершить процесс |

---

## Пример

```elitra
say "Hello, Elitra!"

cell name = "world"
say "Hello, {name}!"

cell x: float = 42
cell items = [1, 2, 3]
cell dict = {"key": "value"}

~fib(n)
    when n <= 1
        emit n
    fib(n-1) + fib(n-2)

shout "fib(10) = {fib(10)}"

over i in 0..5
    say("{i} ")
shout()

dare
    assert(false, "oops!")
catch e
    shout("Caught: {e}")

# pipe operator
1..10 |> filter(\(n) n % 2 == 0) |> map(\(n) n * n) |> collect()

# default params
~repeat(msg, times = 3)
    over _ in 1..times
        shout(msg)

repeat("hi")           # hi hi hi
repeat("hi", 2)        # hi hi

# throw
~div(a, b)
    when b == 0
        throw "cannot divide by zero"
    emit a / b

load "examples/hello.eltr" as ex
shout(type(ex))            # <module>
```
