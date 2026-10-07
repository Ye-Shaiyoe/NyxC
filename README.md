# NyxC

NyxC is a systems-level programming language compiler designed specifically for the **NyxaraOS** userland environment (a 32-bit x86 operating system). The compiler is written entirely in Rust with zero external crate dependencies. It generates GNU x86-32 assembly (`gas`) and compiles it into ELF32 (`elf32-i386`) binary executables.

---

## Key Features

* **Zero External Dependencies**: Built entirely with the Rust standard library (edition 2021), with no third-party libraries.
* **Target Architecture**: x86 32-bit (i386 Protected Mode).
* **Target Format**: ELF32 executable (`entry point 0x04000000`, `cdecl` calling convention).
* **NyxaraOS Syscall ABI**: Trap-gate interrupt interface using `int 0x80` (`eax` = syscall number, `ebx` = arg1, `ecx` = arg2, `edx` = arg3, `esi` = arg4).
* **Type Checking & Mutability**: Supports variable type inference, constants, mutability enforcement (`let` vs `let mut`), pointers, and primitive types.
* **Built-in Functions**:

  * `str_len(s: *u8) -> i32`: Returns the length of a NUL-terminated string (inlined directly during compilation).
  * `print(s: *u8)`: Writes a string to `stdout` without a newline.
  * `println(s: *u8)`: Writes a string to `stdout` followed by a newline (`\n`).
  * `itoa(n: i32) -> *u8`: Converts a signed integer into a NUL-terminated decimal string.
* **Flexible Syntax**: Semicolons (`;`) are optional and can be replaced with newlines.

---

## Data Types & Operators

### Primitive Data Types

* **Signed Integers**: `i8`, `i16`, `i32`
* **Unsigned Integers**: `u8`, `u16`, `u32`
* **Boolean**: `bool` (`true`, `false`)
* **Pointer**: `*T` (e.g. `*u8`, `*i32`, `**u8`)
* **Void**: `void`

### Operators

* **Arithmetic**: `+`, `-`, `*`, `/`, `%`
* **Comparison**: `==`, `!=`, `<`, `<=`, `>`, `>=`
* **Logical & Bitwise**: `&&`, `||`, `!`, `&`, `|`, `^`, `~`, `<<`, `>>`
* **Pointer**: `&var` (address-of), `*ptr` (dereference)

---

## Directory Structure

```text
NyxC/
├── src/
│   ├── ast.rs               # Abstract Syntax Tree (AST) definitions
│   ├── token.rs             # Token definitions & source spans
│   ├── lexer.rs             # Tokenizer / lexical analyzer
│   ├── parser.rs            # Recursive descent parser
│   ├── sema/                # Semantic analysis & symbol tables
│   │   ├── mod.rs           # Type checking & scope verification
│   │   ├── symbols.rs       # Symbol tables, scopes, and constants
│   │   └── types.rs         # Type compatibility & promotions
│   ├── codegen/             # GNU x86 assembly code generator
│   │   ├── mod.rs
│   │   ├── runtime.rs       # Runtime glue (_start & syscall wrappers)
│   │   └── x86_gas.rs       # Assembly emitter (Intel syntax, noprefix)
│   ├── driver.rs            # Compilation orchestration, gas (as), and ld
│   ├── error.rs             # Error diagnostics with caret highlighting
│   ├── target.rs            # Target configuration (nyxara-x86, linux-x86)
│   ├── lib.rs
│   └── main.rs              # CLI interface
├── runtime/
│   └── user.ld              # ELF32 linker script for NyxaraOS userland
├── lib/
│   └── nyx/
│       └── sys.nyx          # NyxaraOS syscall standard library header
├── Learning-NyxC/            # Code examples and learning tutorials
├── docs/                    # Architecture documentation & language specification
└── tests/
    └── compile_tests.rs     # Unit tests and end-to-end integration tests
```

---

## Installation & Requirements

Make sure the following system dependencies are installed on the Linux host:

* **Rust & Cargo** (1.70+)
* **GNU Assembler (`as`)** with 32-bit support (`--32`)
* **GNU Linker (`ld`)** with `elf_i386` emulation

On Arch-based / Gentoo systems:

```bash
# Gentoo / Arch (binutils usually provides 32-bit x86 support)
as --version
ld -V | grep elf_i386
```

---

## CLI Usage Guide

### 1. Check Syntax & Semantics

To verify type correctness and syntax without compiling into a binary:

```bash
cargo run -- check <file.nyx>
```

### 2. Emit x86 Assembly (`.s`)

```bash
cargo run -- emit-asm <file.nyx> -o <output.s>
```

### 3. Build a NyxaraOS ELF32 Binary

```bash
cargo run -- build <file.nyx> -o <output.elf>
```

Additional options:

* `-k`, `--keep-temps`: Keep temporary files (`.s` and `.o`).
* `--target <triple>`: Select the compilation target (default: `nyxara-x86`).

---

## NyxC Code Examples

### Hello World (`hello.nyx`)

```nyx
import "nyx/sys"

fn main() -> i32 {
    sys::write(1, "Hello from NyxC!\n", str_len("Hello from NyxC!\n"))
    return 0
}
```

### Using Built-in Print Functions & a For Loop

```nyx
fn main() -> i32 {
    let mut total = 0
    for (let mut i = 1; i <= 5; i = i + 1) {
        total = total + i
    }

    print("Sum: ")
    println(itoa(total))
    return 0
}
```

---

## Running Tests

Run the complete compiler unit and integration test suite:

```bash
cargo test
```

Run the test suite in release mode:

```bash
cargo test --release
```

---

## Learning Materials

Step-by-step guides and exercises are available in the [`Learning-NyxC/`](Learning-NyxC/README.md) directory.

```

### A couple of small README improvements I made
- Changed `file:///home/akrom/...` to a **relative GitHub-friendly link**: `Learning-NyxC/README.md`.
- Translated comments inside the directory tree too, so the README is consistently English.
- Changed the example output text from `"Hasil penjumlahan: "` to `"Sum: "` so the example is fully English.
- Kept technical names such as **Protected Mode, ELF32, cdecl, trap gate, syscall ABI, GNU Assembler**, etc. unchanged where appropriate.
```
