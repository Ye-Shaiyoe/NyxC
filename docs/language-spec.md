# NyxC Language Specification (v0.1)

## 1. Syntax & Lexical Conventions

### Comments
- Line comment: `// Single-line comment`
- Block comment: `/* Multi-line comment */`

### Semicolons
Semicolons (`;`) are optional. Statements may be terminated by explicit semicolons or newline boundaries.

### Identifiers
`[a-zA-Z_][a-zA-Z0-9_]*`

## 2. Primitive Types
- Integers: `i8`, `i16`, `i32`, `u8`, `u16`, `u32`
- Boolean: `bool` (`true`, `false`)
- Character: `char` (written `'A'`; used as an integral value in expressions)
- Void: `void`
- Pointers: `*T` (e.g. `*u8`, `*i32`, `**u8`)
- Fixed arrays: `[T; N]` (e.g. `[i32; 8]`)
- Structs: user-defined records (`struct P { x: i32, y: i32 }`)

## 3. Declarations

### Functions
```nyx
fn add(a: i32, b: i32) -> i32 {
    return a + b
}

fn log_msg(msg: *u8) {
    sys::write(1, msg, 12)
}
```

### Variables & Constants
```nyx
let a: i32 = 10
let mut counter: i32 = 0
const BUFFER_SIZE: i32 = 1024
```

Type annotations are optional. When omitted, the type is inferred from the
initializer (integers infer to `i32`, string literals to `*u8`):
```nyx
let a = 10
let mut counter = 0
const BUFFER_SIZE = 1024
```

### External Declarations
```nyx
extern fn sys_write(fd: i32, buf: *u8, len: i32) -> i32
```

### Global Variables
Top-level `let` declares a global variable stored in the `.data` section and
addressed absolutely (no EBP frame). The initializer must be a compile-time
constant; without an initializer the storage is zero-filled:
```nyx
let mut counter: i32 = 0
let mut buffer: [u8; 256]
let greeting: *u8 = "ready"
let origin: Point = Point { x: 1, y: 2 }
```

Assigning to a global requires `mut` (same rule as locals). Globals are
shared by all functions, which makes them the natural home for fixed-size
compiler tables.

### Structs and Arrays
```nyx
struct Point { x: i32, y: i32 }

let mut p: Point = Point { x: 1, y: 2 }
p.x = 10

let mut a: [i32; 8]
let mut b: [i32; 4] = [1, 2, 3, 4]
let q: Point = p
```
Indexing `a[i]` and field access `p.x` are first-class expressions. Full
array and whole-struct assignment copies the aggregate.

## 4. Control Flow

### If / Else
```nyx
if x > 0 {
    sys::write(1, "positive\n", 9)
} else {
    sys::write(1, "zero or negative\n", 17)
}
```

### While, For, `break`, and `continue`
```nyx
while n < 10 {
    n = n + 1
    if n == 5 { continue }
}

let i: i32
for (i = 0; i < n; i = i + 1) {
    sum = sum + i
    if sum > 100 { break }
}
```
`break` and `continue` are valid only inside `while` and `for`.

### While
```nyx
while counter < 10 {
    counter = counter + 1
}
```

## 5. Operators
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `~`, `<<`, `>>`
- Pointer operations: `&x` (address-of), `*ptr` (dereference)
- Explicit cast: `x as T` (integral, boolean, or pointer forms)
- Null pointer: `null`
- Compile-time builtin: `sizeof(T)` (size of a type)

## 6. Built-in Functions
Compiler built-ins, always available without an import:

- `str_len(s: *u8) -> i32` — length of a NUL-terminated string, inlined at
  the call site. Removes the need to count string lengths by hand:
```nyx
sys::write(1, "Hello from NyxC!\n", str_len("Hello from NyxC!\n"))
```

## 7. Built-in Syscall Intrinsic
NyxC provides direct low-level hardware interrupt dispatch via `syscall`:
```nyx
syscall(4, 1, msg, len) // eax=4 (SYS_WRITE), ebx=1, ecx=msg, edx=len
```
Or via standard library module `nyx/sys`:
```nyx
import "nyx/sys"

fn main() -> i32 {
    sys::write(1, "Hello from NyxC!\n", str_len("Hello from NyxC!\n"))
    return 0
}
```

## 8. Diagnostics
Compiler errors are structured (`error[CODE]`) and point at the offending
source location with a caret:
```text
error[SEM_001]: cannot find value 'x' in this scope
  --> main.nyx:4:13
  |
4 |     let a = x
  |             ^
```
