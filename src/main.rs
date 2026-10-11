use std::env;
use std::path::{Path, PathBuf};
use std::process;

use nyxc::driver::{Backend, CompileOptions, Driver};
use nyxc::error::{render, CompileError};
use nyxc::lexer::Lexer;
use nyxc::parser::Parser;
use nyxc::sema::SemanticAnalyzer;
use nyxc::target::Target;

fn print_help() {
    println!("NyxC - Modern Systems Programming Language for NyxaraOS");
    println!("Usage:");
    println!("  nyxc build <file.nyx> [options]    Compile NyxC source to NyxaraOS ELF32");
    println!("  nyxc check <file.nyx>              Verify syntax and semantics without linking");
    println!("  nyxc emit-asm <file.nyx>           Compile NyxC source to x86-32 assembly (.s)");
    println!();
    println!("Options:");
    println!("  -o <path>                          Specify output binary path");
    println!("  --target <triple>                  Target triple (default: nyxara-x86)");
    println!("  --backend <gas|native>             Code emitter (default: gas on host, native in-OS)");
    println!("  -k, --keep-temps                   Keep intermediate .s and .o files");
    println!("  -h, --help                         Show help information");
    println!("  -v, --version                      Show version");
}

fn print_compile_error(file: &Path, err: &CompileError) {
    match std::fs::read_to_string(file) {
        Ok(source) => eprint!("{}", render(&file.display().to_string(), &source, err)),
        Err(_) => eprintln!("error[{}]: {}", err.code, err.message),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_help();
        process::exit(1);
    }

    let mut command = "build";
    let mut input_file: Option<PathBuf> = None;
    let mut output_file: Option<PathBuf> = None;
    let mut target = Target::nyxara_x86();
    let mut emit_asm = false;
    let mut keep_intermediates = false;
    let mut backend = Backend::Gas;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-v" | "--version" => {
                println!("nyxc 0.1.0 (target: i386-nyxara)");
                return;
            }
            "build" | "compile" => {
                command = "build";
            }
            "check" => {
                command = "check";
            }
            "emit-asm" => {
                command = "build";
                emit_asm = true;
            }
            "-o" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: -o requires an output path");
                    process::exit(1);
                }
                output_file = Some(PathBuf::from(&args[i]));
            }
            "--target" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --target requires a target name");
                    process::exit(1);
                }
                match Target::from_str(&args[i]) {
                    Ok(t) => target = t,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                }
            }
            "-k" | "--keep-temps" => {
                keep_intermediates = true;
            }
            "--backend" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --backend requires gas or native");
                    process::exit(1);
                }
                backend = match args[i].as_str() {
                    "gas" => Backend::Gas,
                    "native" => Backend::Native,
                    other => {
                        eprintln!("Error: unknown backend '{}' (expected gas or native)", other);
                        process::exit(1);
                    }
                };
            }
            "--emit-asm" => {
                emit_asm = true;
            }
            _ => {
                if !arg.starts_with('-') && input_file.is_none() {
                    input_file = Some(PathBuf::from(arg));
                } else {
                    eprintln!("Unknown argument: {}", arg);
                    process::exit(1);
                }
            }
        }
        i += 1;
    }

    let input = match input_file {
        Some(path) => path,
        None => {
            eprintln!("Error: No input file specified");
            process::exit(1);
        }
    };

    if command == "check" {
        let source = match std::fs::read_to_string(&input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {:?}: {}", input, e);
                process::exit(1);
            }
        };
        let file_display = input.display().to_string();

        let mut lexer = Lexer::new(&source);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                eprint!("{}", render(&file_display, &source, &e));
                process::exit(1);
            }
        };

        let mut parser = Parser::new(tokens);
        let mut program = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                eprint!("{}", render(&file_display, &source, &e));
                process::exit(1);
            }
        };

        let mut analyzer = SemanticAnalyzer::new();
        if let Err(e) = analyzer.analyze_program(&mut program) {
            eprint!("{}", render(&file_display, &source, &e));
            process::exit(1);
        }

        println!("Check passed: {:?} is valid NyxC code.", input);
        return;
    }

    let input_for_err = input.clone();
    let options = CompileOptions {
        input_file: input,
        output_file,
        target,
        emit_asm,
        keep_intermediates,
        backend,
    };

    match Driver::compile(options) {
        Ok(out) => {
            println!("Compiled successfully -> {:?}", out);
        }
        Err(e) => {
            print_compile_error(&input_for_err, &e);
            process::exit(1);
        }
    }
}
