# NyxaraOS Integration Guide

## Embedding NyxC Programs into NyxaraOS

NyxaraOS uses an embedded userland image loaded into memory via `rust/src/initrd.rs` and executed in Ring 3.

### 1. Compile NyxC to NyxaraOS ELF32
Compile any `.nyx` source using the NyxC compiler:
```bash
cd /home/akrom/Projects/NyxC
cargo run --release -- build hello.nyx -o hello.elf            # GNU as/ld backend
cargo run --release -- build hello.nyx -o hello.elf --backend native  # internal encoder
```

### 2. Copy Binary to NyxaraOS Build Directory
Copy the resulting ELF executable to NyxaraOS's build path:
```bash
cp /home/akrom/Projects/NyxC/hello.elf /home/akrom/Projects/NyxaraOS/build/hello.elf
```

### 3. Rebuild and Run NyxaraOS
In the NyxaraOS repository:
```bash
cd /home/akrom/Projects/NyxaraOS
make
make run-serial
```
Inside the NyxaraOS shell, running `hello` will execute your native NyxC userland binary!
