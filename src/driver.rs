use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::codegen::{generate_x86_assembly, native_elf_from_asm};
use crate::error::CompileError;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::sema::SemanticAnalyzer;
use crate::target::{Target, TargetOs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Host GNU as/ld toolchain.
    Gas,
    /// Built-in x86 encoder + ELF32 writer (no host toolchain needed).
    Native,
}

pub struct CompileOptions {
    pub input_file: PathBuf,
    pub output_file: Option<PathBuf>,
    pub target: Target,
    pub emit_asm: bool,
    pub keep_intermediates: bool,
    pub backend: Backend,
}

pub struct Driver;

impl Driver {
    pub fn compile(options: CompileOptions) -> Result<PathBuf, CompileError> {
        let source = fs::read_to_string(&options.input_file).map_err(|e| {
            CompileError::new(
                "IO_001",
                format!("failed to read input file {}: {}", options.input_file.display(), e),
                None,
            )
        })?;

        // 1. Lexer
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize()?;

        // 2. Parser
        let mut parser = Parser::new(tokens);
        let mut program = parser.parse_program()?;

        // 3. Semantic Analysis
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.analyze_program(&mut program)?;

        // 4. Codegen (x86 GNU as assembly)
        let asm_code = generate_x86_assembly(&program, &analyzer)?;

        let stem = options
            .input_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out");

        let parent_dir = options
            .input_file
            .parent()
            .unwrap_or_else(|| Path::new("."));

        let asm_path = if options.emit_asm {
            if let Some(out) = &options.output_file {
                out.clone()
            } else {
                parent_dir.join(format!("{}.s", stem))
            }
        } else {
            parent_dir.join(format!("{}.s", stem))
        };

        if let Some(p) = asm_path.parent() {
            let _ = fs::create_dir_all(p);
        }

        fs::write(&asm_path, &asm_code).map_err(|e| {
            CompileError::new(
                "IO_002",
                format!("failed to write assembly output {}: {}", asm_path.display(), e),
                None,
            )
        })?;

        if options.emit_asm {
            return Ok(asm_path);
        }

        if options.backend == Backend::Native {
            let elf = native_elf_from_asm(&asm_code)?;
            let out_elf = options.output_file.unwrap_or_else(|| {
                parent_dir.join(format!("{}.elf", stem))
            });
            if let Some(p) = out_elf.parent() {
                let _ = fs::create_dir_all(p);
            }
            fs::write(&out_elf, elf).map_err(|e| {
                CompileError::new("IO_002", format!("failed to write ELF output: {}", e), None)
            })?;
            if !options.keep_intermediates {
                let _ = fs::remove_file(&asm_path);
            }
            return Ok(out_elf);
        }

        // 5. Assemble via `as --32`
        let obj_path = parent_dir.join(format!("{}.o", stem));
        let as_output = Command::new("as")
            .arg("--32")
            .arg(&asm_path)
            .arg("-o")
            .arg(&obj_path)
            .output()
            .map_err(|e| {
                CompileError::new("TOOL_001", format!("failed to execute 'as --32': {}", e), None)
            })?;

        if !as_output.status.success() {
            return Err(CompileError::new(
                "TOOL_001",
                format!(
                    "assembler (as --32) failed:\n{}",
                    String::from_utf8_lossy(&as_output.stderr)
                ),
                None,
            ));
        }

        // 6. Link via `ld -m elf_i386`
        let out_elf = options.output_file.unwrap_or_else(|| {
            parent_dir.join(format!("{}.elf", stem))
        });

        if let Some(p) = out_elf.parent() {
            let _ = fs::create_dir_all(p);
        }

        let mut ld_cmd = Command::new("ld");
        ld_cmd.arg("-m").arg("elf_i386");
        ld_cmd.arg("-nostdlib");

        // Write or locate linker script (unique per input to stay race-free)
        let ld_script_content = include_str!("../runtime/user.ld");
        let temp_ld = parent_dir.join(format!(".temp_user_{}_{}.ld", stem, process::id()));
        if options.target.os == TargetOs::Nyxara {
            fs::write(&temp_ld, ld_script_content).map_err(|e| {
                CompileError::new(
                    "IO_002",
                    format!("failed to write temporary linker script: {}", e),
                    None,
                )
            })?;
            ld_cmd.arg("-T").arg(&temp_ld);
        }

        ld_cmd.arg("-o").arg(&out_elf);
        ld_cmd.arg(&obj_path);

        let ld_output = ld_cmd
            .output()
            .map_err(|e| {
                CompileError::new("TOOL_002", format!("failed to execute 'ld -m elf_i386': {}", e), None)
            })?;

        if temp_ld.exists() {
            let _ = fs::remove_file(temp_ld);
        }

        if !ld_output.status.success() {
            return Err(CompileError::new(
                "TOOL_002",
                format!(
                    "linker (ld -m elf_i386) failed:\n{}",
                    String::from_utf8_lossy(&ld_output.stderr)
                ),
                None,
            ));
        }

        if !options.keep_intermediates {
            let _ = fs::remove_file(&asm_path);
            let _ = fs::remove_file(&obj_path);
        }

        Ok(out_elf)
    }
}
