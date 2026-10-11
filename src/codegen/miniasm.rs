use crate::error::CompileError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Text,
    Rodata,
    Data,
}

impl Section {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            ".text" => Some(Section::Text),
            ".rodata" => Some(Section::Rodata),
            ".data" => Some(Section::Data),
            _ => None,
        }
    }
}

pub const USER_BASE: u32 = 0x0400_0000;

pub struct Sections {
    pub text: Vec<u8>,
    pub rodata: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reg {
    Eax,
    Ecx,
    Edx,
    Ebx,
    Esp,
    Ebp,
    Esi,
    Edi,
    Ax,
    Cx,
    Dx,
    Bx,
    Sp,
    Bp,
    Si,
    Di,
    Al,
    Cl,
    Dl,
    Bl,
    Ah,
    Ch,
    Dh,
    Bh,
}

impl Reg {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "eax" => Reg::Eax,
            "ecx" => Reg::Ecx,
            "edx" => Reg::Edx,
            "ebx" => Reg::Ebx,
            "esp" => Reg::Esp,
            "ebp" => Reg::Ebp,
            "esi" => Reg::Esi,
            "edi" => Reg::Edi,
            "ax" => Reg::Ax,
            "cx" => Reg::Cx,
            "dx" => Reg::Dx,
            "bx" => Reg::Bx,
            "sp" => Reg::Sp,
            "bp" => Reg::Bp,
            "si" => Reg::Si,
            "di" => Reg::Di,
            "al" => Reg::Al,
            "cl" => Reg::Cl,
            "dl" => Reg::Dl,
            "bl" => Reg::Bl,
            "ah" => Reg::Ah,
            "ch" => Reg::Ch,
            "dh" => Reg::Dh,
            "bh" => Reg::Bh,
            _ => return None,
        })
    }

    fn code(self) -> u8 {
        match self {
            Reg::Eax | Reg::Ax | Reg::Al => 0,
            Reg::Ecx | Reg::Cx | Reg::Cl => 1,
            Reg::Edx | Reg::Dx | Reg::Dl => 2,
            Reg::Ebx | Reg::Bx | Reg::Bl => 3,
            Reg::Esp | Reg::Sp | Reg::Ah => 4,
            Reg::Ebp | Reg::Bp | Reg::Ch => 5,
            Reg::Esi | Reg::Si | Reg::Dh => 6,
            Reg::Edi | Reg::Di | Reg::Bh => 7,
        }
    }

    fn size(self) -> usize {
        match self {
            Reg::Eax
            | Reg::Ecx
            | Reg::Edx
            | Reg::Ebx
            | Reg::Esp
            | Reg::Ebp
            | Reg::Esi
            | Reg::Edi => 4,
            Reg::Ax | Reg::Cx | Reg::Dx | Reg::Bx | Reg::Sp | Reg::Bp | Reg::Si | Reg::Di => 2,
            _ => 1,
        }
    }
}

#[derive(Clone, Debug)]
enum Operand {
    Reg(Reg),
    Imm(i64),
    Mem(MemOp),
    Label(String),
}

#[derive(Clone, Debug, Default)]
struct MemOp {
    base: Option<Reg>,
    index: Option<Reg>,
    disp: Option<i64>,
    label: Option<String>,
    size: Option<u8>,
}

fn modrm(m: u8, reg: u8, rm: u8) -> u8 {
    (m << 6) | ((reg & 7) << 3) | (rm & 7)
}

fn fits_i8(v: i64) -> bool {
    (-128..=127).contains(&v)
}

fn parse_int(text: &str) -> Option<i64> {
    let t = text.trim().trim_start_matches('(').trim_end_matches(')').trim();
    if let Some(hex) = t.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<i64>().ok()
    }
}

fn parse_operand(text: &str) -> Result<Operand, CompileError> {
    let t = text.trim();
    let mut size_kw = None;
    let mut core = t;
    for kw in ["byte ptr", "word ptr", "dword ptr", "byte", "word", "dword"] {
        if let Some(rest) = core.strip_prefix(kw) {
            size_kw = Some(match kw {
                "byte ptr" | "byte" => 1,
                "word ptr" | "word" => 2,
                _ => 4,
            });
            core = rest.trim();
            break;
        }
    }
    if let Some(inner) = core.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let mut mem = MemOp::default();
        mem.size = size_kw;
        let text = inner.trim();
        for part in text.split('+') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(n) = parse_int(part) {
                mem.disp = Some(n);
            } else if part.starts_with('.') {
                mem.label = Some(part.to_string());
            } else if let Some(r) = Reg::parse(part) {
                if mem.base.is_none() && mem.index.is_none() {
                    mem.base = Some(r);
                } else {
                    mem.index = Some(r);
                }
            } else if part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            {
                mem.label = Some(part.to_string());
            } else {
                return Err(CompileError::new(
                    "GEN_010",
                    format!("cannot assemble operand '{}'", text),
                    None,
                ));
            }
        }
        return Ok(Operand::Mem(mem));
    }
    if let Some(r) = Reg::parse(t) {
        return Ok(Operand::Reg(r));
    }
    if let Some(n) = parse_int(t) {
        return Ok(Operand::Imm(n));
    }
    if t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !t.is_empty()
    {
        return Ok(Operand::Label(t.to_string()));
    }
    Err(CompileError::new(
        "GEN_010",
        format!("cannot assemble operand '{}'", t),
        None,
    ))
}

fn emit_rm(out: &mut Vec<u8>, reg: u8, mem: &MemOp, resolve: &dyn Fn(&str) -> u32) {
    if let Some(label) = &mem.label {
        out.push(modrm(0, reg, 5));
        out.extend_from_slice(&resolve(label).to_le_bytes());
        return;
    }

    let disp = mem.disp;
    let push_disp8 = |out: &mut Vec<u8>, d: i64| out.push(d as i8 as u8);
    let push_disp32 = |out: &mut Vec<u8>, d: i64| out.extend_from_slice(&(d as i32).to_le_bytes());

    match (mem.base, mem.index) {
        (Some(base), None) => {
            let bc = base.code();
            if bc == 4 {
                // esp base requires a SIB byte (index = none)
                match disp {
                    None => {
                        out.push(modrm(0, reg, 4));
                        out.push(0x24);
                    }
                    Some(d) if fits_i8(d) => {
                        out.push(modrm(1, reg, 4));
                        out.push(0x24);
                        push_disp8(out, d);
                    }
                    Some(d) => {
                        out.push(modrm(2, reg, 4));
                        out.push(0x24);
                        push_disp32(out, d);
                    }
                }
            } else if bc == 5 {
                // ebp base always carries a displacement
                match disp {
                    None => {
                        out.push(modrm(1, reg, 5));
                        push_disp8(out, 0);
                    }
                    Some(d) if fits_i8(d) => {
                        out.push(modrm(1, reg, 5));
                        push_disp8(out, d);
                    }
                    Some(d) => {
                        out.push(modrm(2, reg, 5));
                        push_disp32(out, d);
                    }
                }
            } else {
                match disp {
                    None => out.push(modrm(0, reg, bc)),
                    Some(d) if fits_i8(d) => {
                        out.push(modrm(1, reg, bc));
                        push_disp8(out, d);
                    }
                    Some(d) => {
                        out.push(modrm(2, reg, bc));
                        push_disp32(out, d);
                    }
                }
            }
        }
        (Some(base), Some(index)) => {
            let bc = base.code();
            let ic = index.code();
            let ebp_no_disp = bc == 5 && disp.is_none();
            match disp {
                None if !ebp_no_disp => {
                    out.push(modrm(0, reg, 4));
                    out.push((ic & 7) << 3 | (bc & 7));
                }
                Some(d) if fits_i8(d) => {
                    out.push(modrm(1, reg, 4));
                    out.push((ic & 7) << 3 | (bc & 7));
                    push_disp8(out, d);
                }
                other => {
                    let d = other.unwrap_or(0);
                    out.push(modrm(2, reg, 4));
                    out.push((ic & 7) << 3 | (bc & 7));
                    push_disp32(out, d);
                }
            }
        }
        (None, Some(index)) => {
            let ic = index.code();
            out.push(modrm(0, reg, 4));
            out.push((ic & 7) << 3 | 5);
            push_disp32(out, disp.unwrap_or(0));
        }
        (None, None) => {
            out.push(modrm(0, reg, 5));
            push_disp32(out, 0);
        }
    }
}

fn emit_rm_any(out: &mut Vec<u8>, reg: u8, op: &Operand, resolve: &dyn Fn(&str) -> u32) -> Result<(), CompileError> {
    match op {
        Operand::Reg(r) => {
            out.push(modrm(3, reg, r.code()));
            Ok(())
        }
        Operand::Mem(m) => {
            emit_rm(out, reg, m, resolve);
            Ok(())
        }
        _ => Err(CompileError::new("GEN_010", "expected register or memory", None)),
    }
}

fn as_reg(op: &Operand) -> Result<Reg, CompileError> {
    match op {
        Operand::Reg(r) => Ok(*r),
        _ => Err(CompileError::new("GEN_010", "expected a register", None)),
    }
}

fn as_imm(op: &Operand) -> Result<i64, CompileError> {
    match op {
        Operand::Imm(n) => Ok(*n),
        _ => Err(CompileError::new("GEN_010", "expected an immediate", None)),
    }
}

fn as_mem(op: &Operand) -> Result<&MemOp, CompileError> {
    match op {
        Operand::Mem(m) => Ok(m),
        _ => Err(CompileError::new("GEN_010", "expected a memory operand", None)),
    }
}

fn as_label(op: &Operand) -> Result<&str, CompileError> {
    match op {
        Operand::Label(l) => Ok(l),
        _ => Err(CompileError::new("GEN_010", "expected a label", None)),
    }
}

fn alu_x(mnemonic: &str) -> Option<(u8, u8)> {
    // (opcode for reg,rm form; /x field for the imm forms)
    Some(match mnemonic {
        "add" => (0x03, 0),
        "or" => (0x0B, 1),
        "and" => (0x23, 4),
        "sub" => (0x2B, 5),
        "xor" => (0x33, 6),
        "cmp" => (0x3B, 7),
        _ => return None,
    })
}

fn setcc_code(mnemonic: &str) -> Option<u8> {
    Some(match mnemonic {
        "sete" | "setz" => 0x94,
        "setne" | "setnz" => 0x95,
        "setl" => 0x9C,
        "setle" => 0x9E,
        "setg" => 0x9F,
        "setge" => 0x9D,
        "seta" => 0x97,
        "setb" => 0x92,
        _ => return None,
    })
}

fn jcc_code(mnemonic: &str) -> Option<u8> {
    Some(match mnemonic {
        "jo" => 0x80,
        "jno" => 0x81,
        "jb" | "jc" | "jnae" => 0x82,
        "jae" | "jnb" | "jnc" => 0x83,
        "je" | "jz" => 0x84,
        "jne" | "jnz" => 0x85,
        "jbe" | "jna" => 0x86,
        "ja" | "jnbe" => 0x87,
        "js" => 0x88,
        "jns" => 0x89,
        "jp" | "jpe" => 0x8A,
        "jnp" | "jpo" => 0x8B,
        "jl" | "jnge" => 0x8C,
        "jge" | "jnl" => 0x8D,
        "jle" | "jng" => 0x8E,
        "jg" | "jnle" => 0x8F,
        _ => return None,
    })
}

fn encode(
    mnemonic: &str,
    ops: &[Operand],
    cur: u32,
    relax: bool,
    resolve: &dyn Fn(&str) -> u32,
) -> Result<Vec<u8>, CompileError> {
    let mut out = Vec::new();

    let (base_mnem, forced_size) = match mnemonic {
        "movb" => ("mov", Some(1u8)),
        "movw" => ("mov", Some(2u8)),
        "cmpb" => ("cmp", Some(1u8)),
        _ => (mnemonic, None),
    };

    match base_mnem {
        "mov" => {
            let (dst, src) = (&ops[0], &ops[1]);
            let size = forced_size.unwrap_or(match (dst, src) {
                (Operand::Reg(r), Operand::Reg(_)) | (Operand::Reg(r), Operand::Imm(_)) => r.size() as u8,
                (Operand::Reg(r), Operand::Mem(m)) => m.size.unwrap_or(r.size() as u8),
                (Operand::Mem(_), Operand::Reg(r)) => r.size() as u8,
                (Operand::Mem(m), Operand::Imm(_)) => m.size.unwrap_or(4),
                (Operand::Imm(_), Operand::Reg(r)) => r.size() as u8,
                _ => 4,
            });
            if size == 2 {
                out.push(0x66);
            }
            match (dst, src) {
                (Operand::Reg(r), Operand::Imm(n)) => {
                    let n = *n;
                    match size {
                        1 => {
                            out.push(0xB0 + r.code());
                            out.push(n as i8 as u8);
                        }
                        2 => {
                            out.push(0xB8 + r.code());
                            out.extend_from_slice(&(n as u16).to_le_bytes());
                        }
                        _ => {
                            out.push(0xB8 + r.code());
                            out.extend_from_slice(&(n as u32).to_le_bytes());
                        }
                    }
                }
                (Operand::Mem(m), Operand::Imm(n)) => {
                    let n = *n;
                    match size {
                        1 => {
                            out.push(0xC6);
                            emit_rm(&mut out, 0, m, resolve);
                            out.push(n as i8 as u8);
                        }
                        2 => {
                            out.push(0xC7);
                            emit_rm(&mut out, 0, m, resolve);
                            out.extend_from_slice(&(n as u16).to_le_bytes());
                        }
                        _ => {
                            out.push(0xC7);
                            emit_rm(&mut out, 0, m, resolve);
                            out.extend_from_slice(&(n as u32).to_le_bytes());
                        }
                    }
                }
                (Operand::Reg(_), Operand::Reg(_)) => {
                    // mov r/m, r with the destination as r/m (matches GNU as)
                    out.push(0x89);
                    emit_rm_any(&mut out, as_reg(src)?.code(), dst, resolve)?;
                }
                (Operand::Reg(r), Operand::Mem(_)) => {
                    out.push(0x8B);
                    emit_rm_any(&mut out, r.code(), src, resolve)?;
                }
                (Operand::Mem(_), Operand::Reg(r)) => {
                    out.push(if size == 1 { 0x88 } else { 0x89 });
                    emit_rm_any(&mut out, r.code(), dst, resolve)?;
                }
                _ => {
                    return Err(CompileError::new(
                        "GEN_010",
                        format!("unsupported mov operands"),
                        None,
                    ))
                }
            }
        }
        "movzx" => {
            let dst = as_reg(&ops[0])?;
            let (opcode, src) = match &ops[1] {
                Operand::Mem(m) => {
                    let op = match m.size {
                        Some(1) | None => 0xB6,
                        Some(2) => 0xB7,
                        _ => 0xB6,
                    };
                    (op, &ops[1])
                }
                Operand::Reg(r) => (
                    if r.size() == 2 { 0xB7 } else { 0xB6 },
                    &ops[1],
                ),
                _ => {
                    return Err(CompileError::new(
                        "GEN_010",
                        "movzx needs a register or memory source",
                        None,
                    ))
                }
            };
            out.push(0x0F);
            out.push(opcode);
            emit_rm_any(&mut out, dst.code(), src, resolve)?;
        }
        "lea" => {
            let dst = as_reg(&ops[0])?;
            let mem = as_mem(&ops[1])?;
            out.push(0x8D);
            emit_rm(&mut out, dst.code(), mem, resolve);
        }
        "push" => {
            let r = as_reg(&ops[0])?;
            out.push(0x50 + r.code());
        }
        "pop" => {
            let r = as_reg(&ops[0])?;
            out.push(0x58 + r.code());
        }
        "call" => {
            let label = as_label(&ops[0])?;
            out.push(0xE8);
            let target = resolve(label);
            let rel = target as i64 - (cur as i64 + 5);
            out.extend_from_slice(&(rel as i32).to_le_bytes());
        }
        "jmp" => {
            let label = as_label(&ops[0])?;
            let target = resolve(label) as i64;
            let short = target - (cur as i64 + 2);
            if relax && fits_i8(short) {
                out.push(0xEB);
                out.push(short as i8 as u8);
            } else {
                out.push(0xE9);
                let rel = target - (cur as i64 + 5);
                out.extend_from_slice(&(rel as i32).to_le_bytes());
            }
        }
        "ret" => out.push(0xC3),
        "cdq" => out.push(0x99),
        "int" => {
            let n = as_imm(&ops[0])?;
            out.push(0xCD);
            out.push(n as i8 as u8);
        }
        "imul" => {
            let dst = as_reg(&ops[0])?;
            out.push(0x0F);
            out.push(0xAF);
            emit_rm_any(&mut out, dst.code(), &ops[1], resolve)?;
        }
        "idiv" => {
            out.push(0xF7);
            emit_rm_any(&mut out, 7, &ops[0], resolve)?;
        }
        "div" => {
            out.push(0xF7);
            emit_rm_any(&mut out, 6, &ops[0], resolve)?;
        }
        "neg" => {
            out.push(0xF7);
            emit_rm_any(&mut out, 3, &ops[0], resolve)?;
        }
        "not" => {
            out.push(0xF7);
            emit_rm_any(&mut out, 2, &ops[0], resolve)?;
        }
        "test" => {
            out.push(0x85);
            emit_rm_any(&mut out, as_reg(&ops[0])?.code(), &ops[1], resolve)?;
        }
        "inc" => {
            out.push(0x40 + as_reg(&ops[0])?.code());
        }
        "dec" => {
            out.push(0x48 + as_reg(&ops[0])?.code());
        }
        "shl" | "shr" | "sal" | "sar" => {
            let dst = as_reg(&ops[0])?;
            let ext = match base_mnem {
                "shl" | "sal" => 4,
                _ => 5,
            };
            match &ops[1] {
                Operand::Reg(r) if r.code() == 1 => {
                    out.push(0xD3);
                    out.push(modrm(3, ext, dst.code()));
                }
                Operand::Imm(n) => {
                    if *n == 1 {
                        out.push(0xD1);
                        out.push(modrm(3, ext, dst.code()));
                    } else {
                        out.push(0xC1);
                        out.push(modrm(3, ext, dst.code()));
                        out.push(*n as i8 as u8);
                    }
                }
                _ => return Err(CompileError::new("GEN_010", "shift expects cl or an immediate", None)),
            }
        }
        _ => {
            if let Some(cc) = setcc_code(base_mnem) {
                let r = as_reg(&ops[0])?;
                out.push(0x0F);
                out.push(cc);
                out.push(modrm(3, 0, r.code()));
                return Ok(out);
            }
            if let Some((reg_rm, ext)) = alu_x(base_mnem) {
                let (dst, src) = (&ops[0], &ops[1]);
                let size = forced_size.unwrap_or(match (dst, src) {
                    (Operand::Reg(r), Operand::Reg(_)) | (Operand::Reg(r), Operand::Imm(_)) => r.size() as u8,
                    (Operand::Mem(m), _) => m.size.unwrap_or(4),
                    _ => 4,
                });
                if size == 2 {
                    out.push(0x66);
                }
                match (dst, src) {
                    (Operand::Reg(r), Operand::Imm(n)) => {
                        let n = *n;
                        if size == 1 {
                            out.push(0x80);
                            out.push(modrm(3, ext, r.code()));
                            out.push(n as i8 as u8);
                        } else if fits_i8(n) {
                            out.push(0x83);
                            out.push(modrm(3, ext, r.code()));
                            out.push(n as i8 as u8);
                        } else if size == 2 {
                            out.push(0x81);
                            out.push(modrm(3, ext, r.code()));
                            out.extend_from_slice(&(n as u16).to_le_bytes());
                        } else {
                            out.push(0x81);
                            out.push(modrm(3, ext, r.code()));
                            out.extend_from_slice(&(n as u32).to_le_bytes());
                        }
                    }
                    (Operand::Mem(m), Operand::Imm(n)) => {
                        let n = *n;
                        if size == 1 || fits_i8(n) {
                            out.push(if size == 1 { 0x80 } else { 0x83 });
                            emit_rm(&mut out, ext, m, resolve);
                            out.push(n as i8 as u8);
                        } else {
                            out.push(0x81);
                            emit_rm(&mut out, ext, m, resolve);
                            out.extend_from_slice(&(n as u32).to_le_bytes());
                        }
                    }
                    (Operand::Reg(_), Operand::Reg(_)) => {
                        out.push(match base_mnem {
                            "add" => 0x01,
                            "or" => 0x09,
                            "and" => 0x21,
                            "sub" => 0x29,
                            "xor" => 0x31,
                            _ => 0x39,
                        });
                        emit_rm_any(&mut out, as_reg(src)?.code(), dst, resolve)?;
                    }
                    (Operand::Reg(r), _) => {
                        out.push(reg_rm);
                        emit_rm_any(&mut out, r.code(), src, resolve)?;
                    }
                    (Operand::Mem(_), Operand::Reg(r)) => {
                        let base = match base_mnem {
                            "add" => 0x01,
                            "or" => 0x09,
                            "and" => 0x21,
                            "sub" => 0x29,
                            "xor" => 0x31,
                            _ => 0x39,
                        };
                        out.push(if size == 1 { base - 1 } else { base });
                        emit_rm_any(&mut out, r.code(), dst, resolve)?;
                    }
                    _ => return Err(CompileError::new("GEN_010", "unsupported alu operands", None)),
                }
                return Ok(out);
            }
            if let Some(cc) = jcc_code(base_mnem) {
                let label = as_label(&ops[0])?;
                let target = resolve(label) as i64;
                let short = target - (cur as i64 + 2);
                if relax && fits_i8(short) {
                    out.push(0x70 + (cc - 0x80));
                    out.push(short as i8 as u8);
                } else {
                    out.push(0x0F);
                    out.push(cc);
                    let rel = target - (cur as i64 + 6);
                    out.extend_from_slice(&(rel as i32).to_le_bytes());
                }
                return Ok(out);
            }
            return Err(CompileError::new(
                "GEN_010",
                format!("cannot assemble instruction '{}'", mnemonic),
                None,
            ));
        }
    }

    Ok(out)
}

fn unescape_string(text: &str) -> Result<Vec<u8>, CompileError> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push(b'\n'),
                Some('t') => out.push(b'\t'),
                Some('r') => out.push(b'\r'),
                Some('0') => out.push(0),
                Some('\\') => out.push(b'\\'),
                Some('"') => out.push(b'"'),
                Some(other) => out.push(other as u8),
                None => {
                    return Err(CompileError::new(
                        "GEN_010",
                        "unterminated escape in .string",
                        None,
                    ))
                }
            }
        } else {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
    Ok(out)
}

enum Line {
    Label(String),
    Instr(String, Vec<Operand>),
    Bytes(Vec<u8>),
    Space(usize),
    Directive(Section),
}

fn parse_line(line: &str) -> Result<Option<Line>, CompileError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Some(label) = trimmed.strip_suffix(':') {
        let name = label.trim();
        if name.is_empty() {
            return Ok(None);
        }
        return Ok(Some(Line::Label(name.to_string())));
    }

    if let Some(rest) = trimmed.strip_prefix('.') {
        let mut parts = rest.splitn(2, ' ');
        let directive = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();
        match directive {
            "intel_syntax" | "globl" | "global" | "type" | "size" | "align" | "p2align" => Ok(None),
            "section" => {
                let name = arg.split(',').next().unwrap_or("").trim();
                match Section::from_name(name) {
                    Some(section) => Ok(Some(Line::Directive(section))),
                    None => Ok(None),
                }
            }
            "string" | "asciz" => {
                let text = arg
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .ok_or_else(|| {
                        CompileError::new("GEN_010", "malformed .string directive", None)
                    })?;
                let mut bytes = unescape_string(text)?;
                bytes.push(0);
                Ok(Some(Line::Bytes(bytes)))
            }
            "byte" => {
                let mut bytes = Vec::new();
                for part in arg.split(',') {
                    let n = parse_int(part.trim()).ok_or_else(|| {
                        CompileError::new("GEN_010", "malformed .byte directive", None)
                    })?;
                    bytes.push(n as i8 as u8);
                }
                Ok(Some(Line::Bytes(bytes)))
            }
            "space" => {
                let n = parse_int(arg).ok_or_else(|| {
                    CompileError::new("GEN_010", "malformed .space directive", None)
                })?;
                Ok(Some(Line::Space(n.max(0) as usize)))
            }
            _ => Ok(None),
        }
    } else {
        let (mnemonic, rest) = match trimmed.split_once(char::is_whitespace) {
            Some((m, r)) => (m, r),
            None => (trimmed, ""),
        };
        let operands: Result<Vec<Operand>, CompileError> = rest
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| parse_operand(s))
            .collect();
        Ok(Some(Line::Instr(mnemonic.to_string(), operands?)))
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(pos) => &line[..pos],
        None => line,
    }
}

type Labels = std::collections::HashMap<String, (Section, u32)>;

/// Virtual address of an offset inside a section. Sections are laid out
/// contiguously at USER_BASE in text, rodata, data order.
fn address_of(section: Section, offset: u32, section_sizes: &[(Section, u32)]) -> u32 {
    let mut base = USER_BASE;
    for (s, size) in section_sizes {
        if *s == section {
            break;
        }
        base += size;
    }
    base + offset
}

pub fn assemble(source: &str) -> Result<Sections, CompileError> {
    let raw_lines: Vec<&str> = source.lines().map(strip_comment).collect();
    let mut lines: Vec<(Section, Line)> = Vec::new();
    let mut current = Section::Text;

    for raw in raw_lines {
        if let Some(line) = parse_line(raw)? {
            match line {
                Line::Directive(section) => current = section,
                other => lines.push((current, other)),
            }
        }
    }

    // Relaxation loop: jump encodings shrink to the short form as soon as the
    // target fits in an 8-bit displacement, which shifts later addresses, so
    // rounds repeat until section sizes stop changing.
    let mut labels: Labels = Labels::new();
    let mut sizes = [
        (Section::Text, 0u32),
        (Section::Rodata, 0u32),
        (Section::Data, 0u32),
    ];
    for _ in 0..16 {
        let mut round_labels = Labels::new();
        let mut round_sizes = [
            (Section::Text, 0u32),
            (Section::Rodata, 0u32),
            (Section::Data, 0u32),
        ];
        for (section, line) in &lines {
            let idx = round_sizes
                .iter()
                .position(|(s, _)| s == section)
                .expect("known section");
            match line {
                Line::Label(name) => {
                    round_labels.insert(name.clone(), (*section, round_sizes[idx].1));
                }
                Line::Bytes(bytes) => round_sizes[idx].1 += bytes.len() as u32,
                Line::Space(n) => round_sizes[idx].1 += *n as u32,
                Line::Instr(mnemonic, ops) => {
                    let cur = address_of(*section, round_sizes[idx].1, &sizes);
                    let resolver = |name: &str| -> u32 {
                        labels
                            .get(name)
                            .map(|(s, off)| address_of(*s, *off, &sizes))
                            .unwrap_or(0)
                    };
                    let length = encode(mnemonic, ops, cur, true, &resolver)?.len() as u32;
                    round_sizes[idx].1 += length;
                }
                Line::Directive(_) => {}
            }
        }
        let stable = round_sizes == sizes;
        labels = round_labels;
        sizes = round_sizes;
        if stable {
            break;
        }
    }

    let aligned_sizes = [
        (Section::Text, sizes[0].1),
        (Section::Rodata, sizes[1].1),
        (Section::Data, sizes[2].1),
    ];

    let mut sections = Sections {
        text: Vec::new(),
        rodata: Vec::new(),
        data: Vec::new(),
    };

    for (section, line) in &lines {
        let target = match section {
            Section::Text => &mut sections.text,
            Section::Rodata => &mut sections.rodata,
            Section::Data => &mut sections.data,
        };
        let current_len = target.len() as u32;
        match line {
            Line::Label(_) => {}
            Line::Bytes(bytes) => target.extend_from_slice(bytes),
            Line::Space(n) => target.resize(target.len() + n, 0),
            Line::Instr(mnemonic, ops) => {
                let insn_addr = address_of(*section, current_len, &aligned_sizes);
                let resolver = |name: &str| -> u32 {
                    labels
                        .get(name)
                        .map(|(s, off)| address_of(*s, *off, &aligned_sizes))
                        .unwrap_or(0)
                };
                let bytes = encode(mnemonic, ops, insn_addr, true, &resolver)?;
                target.extend_from_slice(&bytes);
            }
            Line::Directive(_) => {}
        }
    }

    sections.text.resize(aligned_sizes[0].1 as usize, 0);
    sections.rodata.resize(aligned_sizes[1].1 as usize, 0);
    sections.data.resize(aligned_sizes[2].1 as usize, 0);

    Ok(sections)
}
