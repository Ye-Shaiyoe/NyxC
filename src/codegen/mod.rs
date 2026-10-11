pub mod elf_writer;
pub mod miniasm;
pub mod runtime;
pub mod x86_gas;

use crate::ast::Program;
use crate::error::CompileError;
use crate::sema::SemanticAnalyzer;
use self::x86_gas::X86GasCodegen;

pub fn generate_x86_assembly(
    program: &Program,
    analyzer: &SemanticAnalyzer,
) -> Result<String, CompileError> {
    let mut codegen = X86GasCodegen::new(analyzer);
    codegen.generate(program)
}

/// Builds a complete ELF32 image from the compiler's own GAS output using the
/// internal encoder, so the same codegen can run where no host assembler and
/// linker exist (e.g. inside NyxaraOS itself).
pub fn native_elf_from_asm(asm_code: &str) -> Result<Vec<u8>, CompileError> {
    let sections = miniasm::assemble(asm_code)?;
    Ok(elf_writer::write_elf32(
        &sections.text,
        &sections.rodata,
        &sections.data,
        miniasm::USER_BASE,
        miniasm::USER_BASE,
    ))
}

pub fn generate_native_elf(
    program: &Program,
    analyzer: &SemanticAnalyzer,
) -> Result<Vec<u8>, CompileError> {
    let asm_code = generate_x86_assembly(program, analyzer)?;
    native_elf_from_asm(&asm_code)
}
