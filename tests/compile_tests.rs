use nyxc::driver::{Backend, CompileOptions, Driver};
use nyxc::lexer::Lexer;
use nyxc::parser::Parser;
use nyxc::sema::SemanticAnalyzer;
use nyxc::target::Target;
use std::fs;

fn parse(src: &str) -> nyxc::ast::Program {
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("tokenize failed");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parse failed")
}

#[test]
fn test_lexer_tokens() {
    let src = "let mut x: i32 = 42\nreturn x";
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("lexer failed");
    assert!(tokens.len() >= 6);
}

#[test]
fn test_parser_basic_function() {
    let src = r#"
        import "nyx/sys"
        fn test_func() -> i32 {
            let a: i32 = 5
            let b: i32 = 10
            return a + b
        }
    "#;
    let prog = parse(src);
    assert_eq!(prog.items.len(), 2);
}

#[test]
fn test_semantic_analysis() {
    let src = r#"
        fn add(a: i32, b: i32) -> i32 {
            return a + b
        }
        fn main() -> i32 {
            return add(10, 20)
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_let_type_inference() {
    let src = r#"
        fn main() -> i32 {
            let a = 5
            let mut b = 10
            let s = "hi"
            b = a + b
            return b
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_const_inferred_type_parses() {
    let prog = parse("const ANSWER = 42\n");
    match &prog.items[0] {
        nyxc::ast::Item::Const(nyxc::ast::Stmt::Const { ty, .. }) => {
            assert!(ty.is_none(), "const type should be inferred, not explicit");
        }
        _ => panic!("expected a const item"),
    }
}

#[test]
fn test_const_explicit_type_still_parses() {
    let prog = parse("const ANSWER: i32 = 42\n");
    match &prog.items[0] {
        nyxc::ast::Item::Const(nyxc::ast::Stmt::Const { ty, .. }) => {
            assert!(matches!(ty.as_ref(), Some(nyxc::ast::Type::I32)));
        }
        _ => panic!("expected a const item"),
    }
}

#[test]
fn test_semantic_error_carries_span() {
    let src = "fn main() -> i32 { let a = missing }\n";
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_001");
    let span = err.span.expect("semantic error must carry a span");
    assert_eq!(span.line, 1);
}

#[test]
fn test_const_usage() {
    let src = r#"
        const ANSWER = 42
        fn main() -> i32 {
            return ANSWER
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_mutability_enforced() {
    let src = "fn main() -> i32 { let x = 10; x = 20; return x }\n";
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_004");
}

#[test]
fn test_for_loop_with_let() {
    let src = r#"
        fn main() -> i32 {
            let mut sum = 0
            for (let mut i = 0; i < 10; i = i + 1) {
                sum = sum + i
            }
            return sum
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_parser_error_carries_span() {
    let mut lexer = Lexer::new("fn {");
    let tokens = lexer.tokenize().expect("tokenize failed");
    let mut parser = Parser::new(tokens);
    let err = parser.parse_program().unwrap_err();
    assert!(err.span.is_some(), "parser error must carry a span");
}

#[test]
fn test_str_len_e2e_elf() {
    fs::create_dir_all("target").expect("failed to create target dir");
    let test_file = "target/str_len_test.nyx";
    let src = r#"
        import "nyx/sys"

        fn main() -> i32 {
            sys::write(1, "str_len ok\n", str_len("str_len ok\n"))
            return 0
        }
    "#;
    fs::write(test_file, src).expect("failed to write test file");

    let options = CompileOptions {
        input_file: test_file.into(),
        output_file: Some("target/str_len_test.elf".into()),
        target: Target::nyxara_x86(),
        emit_asm: false,
        keep_intermediates: true,
        backend: Backend::Gas,
    };

    let result = Driver::compile(options);
    assert!(result.is_ok(), "Driver failed: {:?}", result.err());
    assert!(std::path::Path::new("target/str_len_test.elf").exists());

    let asm = fs::read_to_string("target/str_len_test.s").expect("failed to read .s");
    assert!(asm.contains("cmpb [ebx + ecx], 0"), "inline strlen missing from asm");
}

#[test]
fn test_compile_e2e_elf() {
    let test_file = "target/test_program.nyx";
    let src = r#"
        import "nyx/sys"

        fn main() -> i32 {
            let x: i32 = 1
            if x == 1 {
                sys::write(1, "Test OK\n", 8)
            }
            return 0
        }
    "#;
    fs::write(test_file, src).expect("failed to write test file");

    let options = CompileOptions {
        input_file: test_file.into(),
        output_file: Some("target/test_program.elf".into()),
        target: Target::nyxara_x86(),
        emit_asm: false,
        keep_intermediates: true,
        backend: Backend::Gas,
    };

    let result = Driver::compile(options);
    assert!(result.is_ok(), "Driver failed: {:?}", result.err());
    assert!(std::path::Path::new("target/test_program.elf").exists());
}

#[test]
fn test_itoa_asm_generation() {
    let test_file = "target/itoa_test.nyx";
    let src = r#"
        import "nyx/sys"

        fn main() -> i32 {
            let s = itoa(42)
            sys::write(1, s, 2)
            return 0
        }
    "#;
    fs::write(test_file, src).expect("failed to write test file");

    let options = CompileOptions {
        input_file: test_file.into(),
        output_file: Some("target/itoa_test.elf".into()),
        target: Target::nyxara_x86(),
        emit_asm: false,
        keep_intermediates: true,
        backend: Backend::Gas,
    };

    let result = Driver::compile(options);
    assert!(result.is_ok(), "Driver failed: {:?}", result.err());
    let asm = fs::read_to_string("target/itoa_test.s").expect("failed to read .s");
    assert!(asm.contains(".nyx_itoa:"), "itoa routine missing");
    assert!(asm.contains("div ebx"), "proper division missing from itoa");
}

#[test]
fn test_emit_asm_custom_output() {
    let test_file = "target/custom_asm_test.nyx";
    let out_file = "target/custom_dir/my_asm.s";
    let src = "fn main() -> i32 { return 0 }\n";
    fs::write(test_file, src).expect("failed to write test file");

    let options = CompileOptions {
        input_file: test_file.into(),
        output_file: Some(out_file.into()),
        target: Target::nyxara_x86(),
        emit_asm: true,
        keep_intermediates: false,
        backend: Backend::Gas,
    };

    let result = Driver::compile(options);
    assert!(result.is_ok(), "Driver failed: {:?}", result.err());
    assert_eq!(result.unwrap(), std::path::PathBuf::from(out_file));
    assert!(std::path::Path::new(out_file).exists());
}

#[test]
fn test_cannot_assign_to_const() {
    let src = "const N = 100\nfn main() -> i32 { N = 200; return N }\n";
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_004");
}

#[test]
fn test_addr_of_invalid_expr() {
    let src = "fn main() -> i32 { let p = &(1 + 2); return 0 }\n";
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_013");
}

#[test]
fn test_struct_def_and_field_access() {
    let src = r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() -> i32 {
            let p = Point { x: 3, y: 4 }
            return p.x + p.y
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_struct_unknown_field_rejected() {
    let src = r#"
        struct Point { x: i32 }
        fn main() -> i32 {
            let p = Point { x: 1 }
            return p.z
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_015");
}

#[test]
fn test_struct_literal_missing_field_rejected() {
    let src = r#"
        struct Point { x: i32, y: i32 }
        fn main() -> i32 {
            let p = Point { x: 1 }
            return p.x
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_017");
}

#[test]
fn test_duplicate_struct_rejected() {
    let src = r#"
        struct A { x: i32 }
        struct A { y: i32 }
        fn main() -> i32 { return 0 }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_019");
}

#[test]
fn test_array_index_and_literal() {
    let src = r#"
        fn main() -> i32 {
            let arr: [i32; 3] = [10, 20, 30]
            let mut sum = 0
            for (let mut i = 0; i < 3; i = i + 1) {
                sum = sum + arr[i]
            }
            return sum
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_index_on_non_array_rejected() {
    let src = r#"
        fn main() -> i32 {
            let x = 5
            return x[0]
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_016");
}

#[test]
fn test_immutable_struct_field_rejected() {
    let src = r#"
        struct Point { x: i32 }
        fn main() -> i32 {
            let p = Point { x: 1 }
            p.x = 2
            return p.x
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_004");
}

#[test]
fn test_narrow_int_literal_fits() {
    let src = r#"
        struct Rect { w: i16, h: i16 }
        fn main() -> i32 {
            let r = Rect { w: 10, h: 20 }
            return r.w
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    assert!(sema.analyze_program(&mut prog).is_ok());
}

#[test]
fn test_narrow_int_literal_overflow_rejected() {
    let src = r#"
        struct Rect { w: i16 }
        fn main() -> i32 {
            let r = Rect { w: 100000 }
            return r.w
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    let err = sema.analyze_program(&mut prog).unwrap_err();
    assert_eq!(err.code, "SEM_003");
}

#[test]
fn test_if_block_not_parsed_as_struct_literal() {
    let src = r#"
        const MAX = 100
        fn main() -> i32 {
            let y = 10
            if y < MAX {
                y = 20
            }
            return y
        }
    "#;
    let mut prog = parse(src);
    assert_eq!(prog.items.len(), 2);
}

fn compile_to(src: &str, out: &str, backend: Backend) {
    let file = format!("{}.nyx", out);
    fs::write(&file, src).expect("failed to write test file");
    let options = CompileOptions {
        input_file: file.clone().into(),
        output_file: Some(out.into()),
        target: Target::nyxara_x86(),
        emit_asm: false,
        keep_intermediates: false,
        backend,
    };
    let result = Driver::compile(options);
    assert!(result.is_ok(), "Driver failed: {:?}", result.err());
}

#[test]
fn test_native_backend_matches_ld_layout() {
    let src = r#"
        struct Point {
            x: i32,
            y: i32,
        }

        struct Rect {
            origin: Point,
            w: i16,
            h: i16,
        }

        fn main() -> i32 {
            let mut r = Rect { origin: Point { x: 1, y: 2 }, w: 10, h: 20 }
            r.origin.x = 42
            r.w = 15
            let arr: [i32; 4] = [10, 20, 30, 40]
            let mut sum = 0
            for (let mut k = 0; k < 4; k = k + 1) {
                sum = sum + arr[k]
            }
            println(itoa(r.origin.x + r.w + sum))
            return 0
        }
    "#;
    compile_to(src, "target/native_match_gas.elf", Backend::Gas);
    compile_to(src, "target/native_match_native.elf", Backend::Native);

    let gas = fs::read("target/native_match_gas.elf").expect("gas elf missing");
    let native = fs::read("target/native_match_native.elf").expect("native elf missing");

    let load_payload = |elf: &[u8]| -> Vec<u8> {
        let entry = u32::from_le_bytes([elf[24], elf[25], elf[26], elf[27]]);
        assert_eq!(entry, 0x0400_0000, "entry point must be USER_BASE");
        let phoff = u32::from_le_bytes([elf[28], elf[29], elf[30], elf[31]]) as usize;
        let p_offset = u32::from_le_bytes([
            elf[phoff + 4], elf[phoff + 5], elf[phoff + 6], elf[phoff + 7],
        ]) as usize;
        let p_filesz = u32::from_le_bytes([
            elf[phoff + 16], elf[phoff + 17], elf[phoff + 18], elf[phoff + 19],
        ]) as usize;
        elf[p_offset..p_offset + p_filesz].to_vec()
    };
    let gas_payload = load_payload(&gas);
    let native_payload = load_payload(&native);
    assert!(
        native_payload.starts_with(&gas_payload),
        "native payload must match the ld-produced payload byte-for-byte (gas {} bytes, native {} bytes)",
        gas_payload.len(),
        native_payload.len()
    );
}

#[test]
fn test_stack_frame_covers_deepest_local() {
    // Regression: locals must be recorded at their own EBP offsets before the
    // frame grows, otherwise variables fall outside `sub esp` and get
    // clobbered by the first push.
    let src = r#"
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() -> i32 {
            let p = Point { x: 1, y: 2 }
            let arr: [i32; 3] = [1, 2, 3]
            let mut sum = 0
            for (let mut k = 0; k < 3; k = k + 1) {
                sum = sum + arr[k] + p.x
            }
            return sum
        }
    "#;
    let mut prog = parse(src);
    let mut sema = SemanticAnalyzer::new();
    sema.analyze_program(&mut prog).unwrap();

    for item in &prog.items {
        if let nyxc::ast::Item::Fn(f) = item {
            let vars = sema.symbols.fn_vars(&f.name);
            let deepest = vars.iter().map(|(_, off)| *off).min().unwrap_or(0);
            assert!(
                f.stack_size as i32 + deepest >= 0,
                "stack frame {} does not cover the deepest local at {} in '{}'",
                f.stack_size,
                deepest,
                f.name
            );
        }
    }
}

#[test]
fn test_native_backend_elf_header() {
    compile_to("fn main() -> i32 { return 0 }\n", "target/native_header.elf", Backend::Native);
    let elf = fs::read("target/native_header.elf").expect("native elf missing");
    assert_eq!(&elf[..4], b"\x7fELF");
    assert_eq!(elf[4], 1, "ELFCLASS32");
    assert_eq!(elf[5], 1, "little endian");
    assert_eq!(u16::from_le_bytes([elf[16], elf[17]]), 2, "ET_EXEC");
    assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), 3, "EM_386");
    assert_eq!(u32::from_le_bytes([elf[24], elf[25], elf[26], elf[27]]), 0x0400_0000);
    assert_eq!(u16::from_le_bytes([elf[44], elf[45]]), 1, "phnum");
}
