use crate::ast::*;
use crate::error::CompileError;
use crate::sema::SemanticAnalyzer;
use crate::token::Span;

pub struct X86GasCodegen<'a> {
    analyzer: &'a SemanticAnalyzer,
    strings: Vec<(String, usize)>, // (content, label_id)
    label_counter: usize,
    current_fn_end_label: Option<String>,
    current_fn_name: Option<String>,
    has_itoa: bool,
    has_println: bool,
    loop_stack: Vec<(String, String)>, // (break target, continue target)
}

impl<'a> X86GasCodegen<'a> {
    pub fn new(analyzer: &'a SemanticAnalyzer) -> Self {
        Self {
            analyzer,
            strings: Vec::new(),
            label_counter: 0,
            current_fn_end_label: None,
            current_fn_name: None,
            has_itoa: false,
            has_println: false,
            loop_stack: Vec::new(),
        }
    }

    fn global_label(name: &str) -> String {
        format!("g_{}", name)
    }

    fn lookup_var(&self, name: &str) -> Option<&crate::sema::symbols::Symbol> {
        if let Some(fn_name) = &self.current_fn_name {
            if let Some(sym) = self.analyzer.symbols.lookup_fn_var(fn_name, name) {
                return Some(sym);
            }
        }
        self.analyzer.symbols.lookup_var(name)
    }

    fn new_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!(".L_{}_{}", prefix, self.label_counter)
    }

    fn add_string_literal(&mut self, s: &str) -> usize {
        for (existing, id) in &self.strings {
            if existing == s {
                return *id;
            }
        }
        let id = self.strings.len();
        self.strings.push((s.to_string(), id));
        id
    }

    pub fn generate(&mut self, program: &Program) -> Result<String, CompileError> {
        let mut text_section = String::new();
        let mut has_main = false;
        let mut has_start = false;
        let mut global_data = String::new();

        for item in &program.items {
            if let Item::Global(stmt) = item {
                global_data.push_str(&self.generate_global_decl(stmt)?);
            }
        }

        for item in &program.items {
            if let Item::Fn(f) = item {
                if f.name == "main" {
                    has_main = true;
                }
                if f.name == "_start" {
                    has_start = true;
                }
                if let Some(body) = &f.body {
                    text_section.push_str(&self.generate_fn(f, body)?);
                }
            }
        }

        let mut output = String::new();
        output.push_str(".intel_syntax noprefix\n\n");

        // Read-only data section for string literals
        if !self.strings.is_empty() {
            output.push_str(".section .rodata\n");
            for (s, id) in &self.strings {
                output.push_str(&format!(".LC{}:\n", id));
                output.push_str("    .string ");
                output.push_str(&self.format_asm_string(s));
                output.push('\n');
            }
            output.push('\n');
        }

        // Writable data section: itoa buffer, println newline, then globals
        if self.has_itoa || self.has_println || !global_data.is_empty() {
            output.push_str(".section .data\n");
            if self.has_itoa {
                output.push_str(".nyx_itoa_buf:\n");
                output.push_str("    .space 13\n");
            }
            if self.has_println {
                output.push_str(".nyx_newline:\n");
                output.push_str("    .byte 10\n");
            }
            output.push_str(&global_data);
            output.push('\n');
        }

        // Code section
        output.push_str(".section .text\n");
        output.push_str(&crate::codegen::runtime::generate_runtime_glue(has_main, has_start));
        output.push_str(&text_section);

        if self.has_itoa {
            output.push_str(Self::itoa_routine());
        }

        Ok(output)
    }

    fn itoa_routine() -> &'static str {
        // arg n in [esp+16]; result pointer in eax
        "
.nyx_itoa:
    push ebx
    push esi
    push edi
    lea esi, [.nyx_itoa_buf]
    add esi, 12
    movb [esi], 0
    mov eax, [esp + 16]
    mov ecx, 0
    test eax, eax
    jns .nyx_itoa_abs
    mov ecx, 1
    neg eax
.nyx_itoa_abs:
    mov ebx, 10
.nyx_itoa_digit:
    mov edx, 0
    div ebx
    add dl, 48
    dec esi
    movb [esi], dl
    test eax, eax
    jnz .nyx_itoa_digit
    test ecx, ecx
    jz .nyx_itoa_done
    dec esi
    movb [esi], 45
.nyx_itoa_done:
    mov eax, esi
    pop edi
    pop esi
    pop ebx
    ret
"
    }

    fn format_asm_string(&self, s: &str) -> String {
        let mut res = String::from("\"");
        for ch in s.chars() {
            match ch {
                '\n' => res.push_str("\\n"),
                '\t' => res.push_str("\\t"),
                '\r' => res.push_str("\\r"),
                '\0' => res.push_str("\\0"),
                '\\' => res.push_str("\\\\"),
                '"' => res.push_str("\\\""),
                _ => res.push(ch),
            }
        }
        res.push('"');
        res
    }

    fn generate_fn(&mut self, def: &FnDef, body: &[Stmt]) -> Result<String, CompileError> {
        let mut asm = String::new();
        let fn_label = def.name.clone();
        let end_label = format!(".L_end_{}", fn_label);
        self.current_fn_end_label = Some(end_label.clone());
        self.current_fn_name = Some(def.name.clone());

        asm.push_str(&format!(".globl {}\n", fn_label));
        asm.push_str(&format!("{}:\n", fn_label));
        asm.push_str("    push ebp\n");
        asm.push_str("    mov ebp, esp\n");
        if def.stack_size > 0 {
            asm.push_str(&format!("    sub esp, {}\n", def.stack_size));
        }

        for stmt in body {
            asm.push_str(&self.generate_stmt(stmt)?);
        }

        asm.push_str(&format!("{}:\n", end_label));
        asm.push_str("    mov esp, ebp\n");
        asm.push_str("    pop ebp\n");
        asm.push_str("    ret\n\n");

        self.current_fn_end_label = None;
        self.current_fn_name = None;
        Ok(asm)
    }

    fn generate_stmt(&mut self, stmt: &Stmt) -> Result<String, CompileError> {
        let mut asm = String::new();

        match stmt {
            Stmt::Let { name, init, .. } => {
                if let Some(init_expr) = init {
                    match init_expr {
                        Expr::StructLit(struct_name, inits, _) => {
                            if let Some(sym) = self.lookup_var(name) {
                                self.generate_struct_lit(sym.offset, struct_name, inits, &mut asm)?;
                            }
                        }
                        Expr::ArrayLit(elems, _) => {
                            let sym = self.lookup_var(name).cloned();
                            if let Some(sym) = sym {
                                if let Type::Array(elem, _) = sym.ty.clone() {
                                    self.generate_array_lit(sym.offset, &elem, elems, &mut asm)?;
                                }
                            }
                        }
                        _ => {
                            let init_ty = self.type_of(init_expr)?;
                            if init_ty.size_in_bytes(&self.analyzer.symbols.structs) > 4 {
                                if let Some(sym) = self.lookup_var(name) {
                                    self.copy_into_slot(
                                        &format!("[ebp + ({})]", sym.offset),
                                        init_expr,
                                        &mut asm,
                                    )?;
                                }
                            } else {
                                asm.push_str(&self.generate_expr(init_expr)?);
                                if let Some(sym) = self.lookup_var(name) {
                                    asm.push_str(&format!(
                                        "    mov [ebp + ({})], eax\n",
                                        sym.offset
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    asm.push_str(&self.generate_expr(expr)?);
                }
                if let Some(end_label) = &self.current_fn_end_label {
                    asm.push_str(&format!("    jmp {}\n", end_label));
                }
            }
            Stmt::Expr(expr, _) => {
                asm.push_str(&self.generate_expr(expr)?);
            }
            Stmt::If { cond, then_branch, else_branch, .. } => {
                let else_lbl = self.new_label("else");
                let end_lbl = self.new_label("endif");

                asm.push_str(&self.generate_expr(cond)?);
                asm.push_str("    test eax, eax\n");
                asm.push_str(&format!("    jz {}\n", if else_branch.is_some() { &else_lbl } else { &end_lbl }));

                for s in then_branch {
                    asm.push_str(&self.generate_stmt(s)?);
                }

                if let Some(else_stmts) = else_branch {
                    asm.push_str(&format!("    jmp {}\n", end_lbl));
                    asm.push_str(&format!("{}:\n", else_lbl));
                    for s in else_stmts {
                        asm.push_str(&self.generate_stmt(s)?);
                    }
                }

                asm.push_str(&format!("{}:\n", end_lbl));
            }
            Stmt::While { cond, body, .. } => {
                let loop_start = self.new_label("while_start");
                let loop_end = self.new_label("while_end");

                asm.push_str(&format!("{}:\n", loop_start));
                asm.push_str(&self.generate_expr(cond)?);
                asm.push_str("    test eax, eax\n");
                asm.push_str(&format!("    jz {}\n", loop_end));

                self.loop_stack.push((loop_end.clone(), loop_start.clone()));
                for s in body {
                    asm.push_str(&self.generate_stmt(s)?);
                }
                self.loop_stack.pop();

                asm.push_str(&format!("    jmp {}\n", loop_start));
                asm.push_str(&format!("{}:\n", loop_end));
            }
            Stmt::Break { .. } => {
                let target = self
                    .loop_stack
                    .last()
                    .map(|(end, _)| end.clone())
                    .ok_or_else(|| {
                        CompileError::new("SEM_020", "break outside of a loop", None)
                    })?;
                asm.push_str(&format!("    jmp {}\n", target));
            }
            Stmt::Continue { .. } => {
                let target = self
                    .loop_stack
                    .last()
                    .map(|(_, step)| step.clone())
                    .ok_or_else(|| {
                        CompileError::new("SEM_020", "continue outside of a loop", None)
                    })?;
                asm.push_str(&format!("    jmp {}\n", target));
            }
            Stmt::Block(stmts, _) => {
                for s in stmts {
                    asm.push_str(&self.generate_stmt(s)?);
                }
            }
            Stmt::For { init, cond, step, body, .. } => {
                let loop_start = self.new_label("for_start");
                let loop_step = self.new_label("for_step");
                let loop_end = self.new_label("for_end");

                // Execute init statement once (before the loop)
                asm.push_str(&self.generate_stmt(init)?);

                // Condition check
                asm.push_str(&format!("{}:\n", loop_start));
                asm.push_str(&self.generate_expr(cond)?);
                asm.push_str("    test eax, eax\n");
                asm.push_str(&format!("    jz {}\n", loop_end));

                // Body
                self.loop_stack.push((loop_end.clone(), loop_step.clone()));
                for s in body {
                    asm.push_str(&self.generate_stmt(s)?);
                }
                self.loop_stack.pop();

                // Step expression (e.g., increment), then back to the condition
                asm.push_str(&format!("{}:\n", loop_step));
                asm.push_str(&self.generate_expr(step)?);
                asm.push_str(&format!("    jmp {}\n", loop_start));
                asm.push_str(&format!("{}:\n", loop_end));
            }
            Stmt::LetWrapper(inner) => {
                asm.push_str(&self.generate_stmt(inner.as_ref())?);
            }
            Stmt::ExprInit(expr, _) => {
                asm.push_str(&self.generate_expr(expr.as_ref())?);
            }
            Stmt::Const { .. } => {}
        }

        Ok(asm)
    }

    fn generate_expr(&mut self, expr: &Expr) -> Result<String, CompileError> {
        let mut asm = String::new();

        match expr {
            Expr::IntLit(n, _) => {
                asm.push_str(&format!("    mov eax, {}\n", n));
            }
            Expr::BoolLit(b, _) => {
                asm.push_str(&format!("    mov eax, {}\n", if *b { 1 } else { 0 }));
            }
            Expr::Null(_) => {
                asm.push_str("    mov eax, 0\n");
            }
            Expr::Sizeof(ty, span) => {
                let size = ty.size_in_bytes(&self.analyzer.symbols.structs) as i32;
                if size == 0 {
                    return Err(CompileError::new(
                        "GEN_013",
                        "sizeof cannot be evaluated to zero bytes",
                        Some(*span),
                    ));
                }
                asm.push_str(&format!("    mov eax, {}\n", size));
            }
            Expr::Cast(expr, target, _) => {
                let mut inner = self.generate_expr(expr)?;
                match target {
                    Type::Bool => {
                        inner.push_str("    test eax, eax\n");
                        inner.push_str("    setne al\n");
                        inner.push_str("    movzx eax, al\n");
                    }
                    Type::I8 => {
                        inner.push_str("    movsx eax, al\n");
                    }
                    Type::U8 => {
                        inner.push_str("    and eax, 0xFF\n");
                    }
                    Type::I16 => {
                        inner.push_str("    movsx eax, ax\n");
                    }
                    Type::U16 => {
                        inner.push_str("    and eax, 0xFFFF\n");
                    }
                    _ => {}
                }
                asm = inner;
            }
            Expr::StringLit(s, _) => {
                let str_id = self.add_string_literal(s);
                asm.push_str(&format!("    lea eax, [.LC{}]\n", str_id));
            }
            Expr::Ident(name, span) => {
                if let Some(sym) = self.lookup_var(name) {
                    asm.push_str(&format!("    mov eax, [ebp + ({})]\n", sym.offset));
                } else if let Some(g) = self.analyzer.symbols.lookup_global(name) {
                    asm.push_str(&format!(
                        "    mov eax, [{}]\n",
                        Self::global_label(&g.name)
                    ));
                } else if let Some(cs) = self.analyzer.symbols.lookup_const(name) {
                    use crate::sema::symbols::ConstValue;
                    match &cs.value {
                        ConstValue::Int(n) => asm.push_str(&format!("    mov eax, {}\n", n)),
                        ConstValue::Bool(b) => {
                            asm.push_str(&format!("    mov eax, {}\n", if *b { 1 } else { 0 }))
                        }
                        ConstValue::Str(s) => {
                            let str_id = self.add_string_literal(s);
                            asm.push_str(&format!("    lea eax, [.LC{}]\n", str_id));
                        }
                        ConstValue::Array(_) | ConstValue::Struct(_) => {
                            return Err(CompileError::new(
                                "GEN_012",
                                format!(
                                    "aggregate constant '{}' cannot be used as a scalar value",
                                    name
                                ),
                                Some(*span),
                            ));
                        }
                    }
                } else {
                    return Err(CompileError::new(
                        "GEN_002",
                        format!("undefined variable '{}'", name),
                        Some(*span),
                    ));
                }
            }
            Expr::Path(parts, span) => {
                let full_name = parts.join("::");
                return Err(CompileError::new(
                    "GEN_003",
                    format!("path expression '{}' cannot be used as a value", full_name),
                    Some(*span),
                ));
            }
            Expr::Field(base, field, span) => {
                let ty = self.type_of(base)?;
                let (foff, fty) = self.field_info_from_type(&ty, field, *span)?;
                match ty {
                    Type::Struct(_) => self.generate_addr(base, &mut asm)?,
                    Type::Pointer(_) => asm.push_str(&self.generate_expr(base)?),
                    _ => unreachable!(),
                }
                self.load_at(&fty, "eax", foff as i32, &mut asm);
            }
            Expr::Index(base, _idx, span) => {
                let elem_ty = match self.type_of(base)? {
                    Type::Array(elem, _) | Type::Pointer(elem) => *elem,
                    other => {
                        return Err(CompileError::new(
                            "GEN_008",
                            format!("cannot index value of type {}", other.name()),
                            Some(*span),
                        ))
                    }
                };
                let elem_size = elem_ty.size_in_bytes(&self.analyzer.symbols.structs);
                self.generate_index_addr(base, _idx, elem_size, *span, &mut asm)?;
                self.load_at(&elem_ty, "eax", 0, &mut asm);
            }
            Expr::StructLit(_, _, span) | Expr::ArrayLit(_, span) => {
                return Err(CompileError::new(
                    "GEN_006",
                    "aggregate literals are only supported as let initializers or assignment values",
                    Some(*span),
                ))
            }
            Expr::Assign { target, value, span } => {
                if let (Expr::Ident(name, _), Expr::StructLit(struct_name, inits, _)) =
                    (&**target, &**value)
                {
                    if let Some(sym) = self.lookup_var(name) {
                        self.generate_struct_lit(sym.offset, struct_name, inits, &mut asm)?;
                        return Ok(asm);
                    }
                }
                if let (Expr::Ident(name, _), Expr::ArrayLit(elems, _)) =
                    (&**target, &**value)
                {
                    let sym = self.lookup_var(name).cloned();
                    if let Some(sym) = sym {
                        if let Type::Array(elem, _) = sym.ty.clone() {
                            self.generate_array_lit(sym.offset, &elem, elems, &mut asm)?;
                            return Ok(asm);
                        }
                    }
                }
                if matches!(&**value, Expr::StructLit(..) | Expr::ArrayLit(..)) {
                    self.generate_aggregate_assign(target, value, &mut asm)?;
                    return Ok(asm);
                }
                let target_ty = self.type_of(target)?;
                if target_ty.size_in_bytes(&self.analyzer.symbols.structs) > 4 {
                    self.copy_aggregate_assign(target, value, &mut asm)?;
                    return Ok(asm);
                }
                asm.push_str(&self.generate_expr(value)?);
                match &**target {
                    Expr::Ident(name, target_span) => {
                        if let Some(sym) = self.lookup_var(name) {
                            asm.push_str(&format!("    mov [ebp + ({})], eax\n", sym.offset));
                        } else if let Some(g) = self.analyzer.symbols.lookup_global(name) {
                            asm.push_str(&format!(
                                "    mov [{}], eax\n",
                                Self::global_label(&g.name)
                            ));
                        } else {
                            return Err(CompileError::new(
                                "GEN_002",
                                format!("undefined assignment target '{}'", name),
                                Some(*target_span),
                            ));
                        }
                    }
                    Expr::Unary(UnaryOp::Deref, ptr_expr, _) => {
                        asm.push_str("    push eax\n");
                        asm.push_str(&self.generate_expr(ptr_expr)?);
                        asm.push_str("    mov edx, eax\n");
                        asm.push_str("    pop eax\n");
                        asm.push_str("    mov [edx], eax\n");
                    }
                    Expr::Field(..) | Expr::Index(..) => {
                        let target_ty = self.type_of(target)?;
                        asm.push_str("    push eax\n");
                        self.generate_addr(target, &mut asm)?;
                        asm.push_str("    mov edx, eax\n");
                        asm.push_str("    pop eax\n");
                        self.emit_store(&target_ty, "[edx]", &mut asm);
                    }
                    _ => {
                        return Err(CompileError::new(
                            "GEN_004",
                            "invalid assignment target",
                            Some(*span),
                        ))
                    }
                }
            }
            Expr::Unary(UnaryOp::AddrOf, inner, span) => {
                match &**inner {
                    Expr::Ident(name, ident_span) => {
                        if let Some(sym) = self.lookup_var(name) {
                            asm.push_str(&format!("    lea eax, [ebp + ({})]\n", sym.offset));
                        } else if let Some(g) = self.analyzer.symbols.lookup_global(name) {
                            asm.push_str(&format!(
                                "    lea eax, [{}]\n",
                                Self::global_label(&g.name)
                            ));
                        } else {
                            return Err(CompileError::new(
                                "GEN_002",
                                format!("undefined variable '{}'", name),
                                Some(*ident_span),
                            ));
                        }
                    }
                    Expr::Unary(UnaryOp::Deref, ptr_expr, _) => {
                        asm.push_str(&self.generate_expr(ptr_expr)?);
                    }
                    Expr::Field(..) | Expr::Index(..) => {
                        self.generate_addr(inner, &mut asm)?;
                    }
                    _ => {
                        return Err(CompileError::new(
                            "GEN_004",
                            "cannot take address of non-variable expression",
                            Some(*span),
                        ));
                    }
                }
            }
            Expr::Unary(op, inner, _) => {
                asm.push_str(&self.generate_expr(inner)?);
                match op {
                    UnaryOp::Neg => asm.push_str("    neg eax\n"),
                    UnaryOp::Not => {
                        asm.push_str("    test eax, eax\n");
                        asm.push_str("    sete al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    UnaryOp::BitNot => asm.push_str("    not eax\n"),
                    UnaryOp::Deref => asm.push_str("    mov eax, [eax]\n"),
                    UnaryOp::AddrOf => unreachable!(),
                }
            }
            Expr::Binary(op, left, right, _) => {
                asm.push_str(&self.generate_expr(left)?);
                asm.push_str("    push eax\n");
                asm.push_str(&self.generate_expr(right)?);
                asm.push_str("    mov ebx, eax\n");
                asm.push_str("    pop eax\n");

                match op {
                    BinaryOp::Add => asm.push_str("    add eax, ebx\n"),
                    BinaryOp::Sub => asm.push_str("    sub eax, ebx\n"),
                    BinaryOp::Mul => asm.push_str("    imul eax, ebx\n"),
                    BinaryOp::Div => {
                        asm.push_str("    cdq\n");
                        asm.push_str("    idiv ebx\n");
                    }
                    BinaryOp::Mod => {
                        asm.push_str("    cdq\n");
                        asm.push_str("    idiv ebx\n");
                        asm.push_str("    mov eax, edx\n");
                    }
                    BinaryOp::Eq => {
                        asm.push_str("    cmp eax, ebx\n");
                        asm.push_str("    sete al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    BinaryOp::Ne => {
                        asm.push_str("    cmp eax, ebx\n");
                        asm.push_str("    setne al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    BinaryOp::Lt => {
                        asm.push_str("    cmp eax, ebx\n");
                        asm.push_str("    setl al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    BinaryOp::Le => {
                        asm.push_str("    cmp eax, ebx\n");
                        asm.push_str("    setle al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    BinaryOp::Gt => {
                        asm.push_str("    cmp eax, ebx\n");
                        asm.push_str("    setg al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    BinaryOp::Ge => {
                        asm.push_str("    cmp eax, ebx\n");
                        asm.push_str("    setge al\n");
                        asm.push_str("    movzx eax, al\n");
                    }
                    BinaryOp::BitAnd | BinaryOp::And => asm.push_str("    and eax, ebx\n"),
                    BinaryOp::BitOr | BinaryOp::Or => asm.push_str("    or eax, ebx\n"),
                    BinaryOp::BitXor => asm.push_str("    xor eax, ebx\n"),
                    BinaryOp::Shl => {
                        asm.push_str("    mov ecx, ebx\n");
                        asm.push_str("    shl eax, cl\n");
                    }
                    BinaryOp::Shr => {
                        asm.push_str("    mov ecx, ebx\n");
                        asm.push_str("    shr eax, cl\n");
                    }
                }
            }
            Expr::Call { callee, args, span } => {
                let target_label = match &**callee {
                    Expr::Ident(name, _) => name.clone(),
                    Expr::Path(parts, _) => parts.join("_"), // sys::write -> sys_write
                    _ => {
                        return Err(CompileError::new(
                            "GEN_005",
                            "function call target must be an identifier or a path",
                            Some(*span),
                        ))
                    }
                };

                if target_label == "str_len" {
                    self.generate_str_len(&args[0], &mut asm)?;
                } else if target_label == "print" {
                    self.generate_print(&args[0], false, &mut asm)?;
                } else if target_label == "println" {
                    self.has_println = true;
                    self.generate_print(&args[0], true, &mut asm)?;
                } else if target_label == "itoa" {
                    self.has_itoa = true;
                    for arg in args.iter().rev() {
                        asm.push_str(&self.generate_expr(arg)?);
                        asm.push_str("    push eax\n");
                    }
                    asm.push_str("    call .nyx_itoa\n");
                    asm.push_str(&format!("    add esp, {}\n", args.len() * 4));
                } else {
                    // Push arguments right-to-left
                    for arg in args.iter().rev() {
                        asm.push_str(&self.generate_expr(arg)?);
                        asm.push_str("    push eax\n");
                    }

                    asm.push_str(&format!("    call {}\n", target_label));
                    if !args.is_empty() {
                        asm.push_str(&format!("    add esp, {}\n", args.len() * 4));
                    }
                }
            }
            Expr::Syscall { number, args, .. } => {
                // Intrinsic direct syscall
                let registers = ["ebx", "ecx", "edx", "esi"];
                for (i, arg) in args.iter().enumerate().take(registers.len()) {
                    asm.push_str(&self.generate_expr(arg)?);
                    asm.push_str(&format!("    push eax # save {}\n", registers[i]));
                }
                // Syscall number into eax
                asm.push_str(&self.generate_expr(number)?);
                // Pop saved registers into target registers in reverse
                for (i, _) in args.iter().enumerate().take(registers.len()).rev() {
                    asm.push_str(&format!("    pop {}\n", registers[i]));
                }
                asm.push_str("    int 0x80\n");
            }
        }

        Ok(asm)
    }

    // Inline NUL-terminated string length; clobbers ebx/ecx (caller-saved in cdecl)
    fn generate_str_len(&mut self, arg: &Expr, asm: &mut String) -> Result<(), CompileError> {
        asm.push_str(&self.generate_expr(arg)?);
        let loop_lbl = self.new_label("len");
        let done_lbl = self.new_label("len_done");
        asm.push_str(&format!(
            "    mov ebx, eax\n    mov ecx, 0\n{}:\n    cmpb [ebx + ecx], 0\n    je {}\n    inc ecx\n    jmp {}\n{}:\n    mov eax, ecx\n",
            loop_lbl, done_lbl, loop_lbl, done_lbl
        ));
        Ok(())
    }

    // Inline sys_write(1, s, str_len(s)); if newline=true, also writes the newline byte.
    // Clobbers eax/ebx/ecx/edx (all caller-saved in cdecl).
    fn generate_print(&mut self, arg: &Expr, newline: bool, asm: &mut String) -> Result<(), CompileError> {
        // Evaluate arg -> eax (pointer to string)
        asm.push_str(&self.generate_expr(arg)?);
        // Save pointer: ebx = s
        asm.push_str("    mov ebx, eax\n");
        // Inline str_len: ecx = length
        let loop_lbl = self.new_label("print_len");
        let done_lbl = self.new_label("print_len_done");
        asm.push_str(&format!(
            "    mov ecx, 0\n{}:\n    cmpb [ebx + ecx], 0\n    je {}\n    inc ecx\n    jmp {}\n{}:\n",
            loop_lbl, done_lbl, loop_lbl, done_lbl
        ));
        // sys_write(1, s, len): eax=4, ebx=1(fd), ecx=ptr, edx=len
        // We have: ebx=ptr, ecx=len — shuffle into ABI registers
        asm.push_str("    mov edx, ecx\n"); // edx = len
        asm.push_str("    mov ecx, ebx\n"); // ecx = ptr
        asm.push_str("    mov ebx, 1\n");   // ebx = stdout
        asm.push_str("    mov eax, 4\n");   // eax = SYS_WRITE
        asm.push_str("    int 0x80\n");
        if newline {
            // sys_write(1, &newline_byte, 1)
            asm.push_str("    lea ecx, [.nyx_newline]\n");
            asm.push_str("    mov ebx, 1\n");
            asm.push_str("    mov edx, 1\n");
            asm.push_str("    mov eax, 4\n");
            asm.push_str("    int 0x80\n");
        }
        Ok(())
    }

    fn type_of(&self, expr: &Expr) -> Result<Type, CompileError> {
        match expr {
            Expr::IntLit(..) | Expr::StringLit(..) | Expr::BoolLit(..) | Expr::Null(_) => Ok(match expr {
                Expr::StringLit(..) => Type::Pointer(Box::new(Type::U8)),
                Expr::BoolLit(..) => Type::Bool,
                Expr::Null(_) => Type::Pointer(Box::new(Type::U8)),
                _ => Type::I32,
            }),
            Expr::Sizeof(_, _) => Ok(Type::I32),
            Expr::Cast(_, target, _) => Ok(target.clone()),
            Expr::Ident(name, span) => self
                .lookup_var(name)
                .map(|sym| sym.ty.clone())
                .or_else(|| {
                    self.analyzer
                        .symbols
                        .lookup_const(name)
                        .map(|cs| cs.ty.clone())
                })
                .or_else(|| {
                    self.analyzer
                        .symbols
                        .lookup_global(name)
                        .map(|g| g.ty.clone())
                })
                .ok_or_else(|| {
                    CompileError::new("GEN_001", format!("unknown variable '{}'", name), Some(*span))
                }),
            Expr::Field(base, field, span) => {
                let ty = self.type_of(base)?;
                let (foff, fty) = self.field_info_from_type(&ty, field, *span)?;
                let _ = foff;
                Ok(fty)
            }
            Expr::Index(base, _, span) => match self.type_of(base)? {
                Type::Array(elem, _) | Type::Pointer(elem) => Ok(*elem),
                other => Err(CompileError::new(
                    "GEN_008",
                    format!("cannot index value of type {}", other.name()),
                    Some(*span),
                )),
            },
            Expr::Unary(UnaryOp::Deref, inner, span) => match self.type_of(inner)? {
                Type::Pointer(elem) => Ok(*elem),
                other => Err(CompileError::new(
                    "SEM_010",
                    format!("cannot dereference value of type {}", other.name()),
                    Some(*span),
                )),
            },
            Expr::Unary(UnaryOp::AddrOf, inner, _) => {
                Ok(Type::Pointer(Box::new(self.type_of(inner)?)))
            }
            Expr::Unary(UnaryOp::Not, ..) => Ok(Type::Bool),
            Expr::Unary(_, inner, _) => self.type_of(inner),
            Expr::Binary(op, left, right, _) => {
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::And
                        | BinaryOp::Or
                ) {
                    Ok(Type::Bool)
                } else {
                    let lty = self.type_of(left)?;
                    let rty = self.type_of(right)?;
                    if lty == rty {
                        Ok(lty)
                    } else if matches!(lty, Type::I32 | Type::U32) {
                        Ok(lty)
                    } else {
                        Ok(rty)
                    }
                }
            }
            Expr::Call { callee, span, .. } => {
                let name = match &**callee {
                    Expr::Ident(name, _) => name.clone(),
                    Expr::Path(parts, _) => parts.join("_"),
                    _ => {
                        return Err(CompileError::new(
                            "GEN_005",
                            "function call target must be an identifier or a path",
                            Some(*span),
                        ))
                    }
                };
                self.analyzer
                    .symbols
                    .lookup_fn(&name)
                    .map(|f| f.ret_type.clone())
                    .ok_or_else(|| {
                        CompileError::new(
                            "SEM_002",
                            format!("cannot find function '{}'", name),
                            Some(*span),
                        )
                    })
            }
            Expr::Assign { target, .. } => self.type_of(target),
            Expr::StructLit(name, _, span) => {
                if self.analyzer.symbols.lookup_struct(name).is_none() {
                    return Err(CompileError::new(
                        "SEM_018",
                        format!("unknown struct '{}'", name),
                        Some(*span),
                    ));
                }
                Ok(Type::Struct(name.clone()))
            }
            Expr::ArrayLit(elems, span) => {
                if elems.is_empty() {
                    return Err(CompileError::new(
                        "SEM_018",
                        "empty array literal",
                        Some(*span),
                    ));
                }
                let elem_ty = self.type_of(&elems[0])?;
                Ok(Type::Array(Box::new(elem_ty), elems.len()))
            }
            Expr::Syscall { .. } => Ok(Type::I32),
            Expr::Path(_, span) => Err(CompileError::new(
                "GEN_003",
                "path expression cannot be used as a value",
                Some(*span),
            )),
        }
    }

    fn field_info_from_type(
        &self,
        ty: &Type,
        field: &str,
        span: Span,
    ) -> Result<(u32, Type), CompileError> {
        let struct_name = match ty {
            Type::Struct(name) => name.clone(),
            Type::Pointer(inner) if matches!(**inner, Type::Struct(_)) => match **inner {
                Type::Struct(ref name) => name.clone(),
                _ => unreachable!(),
            },
            other => {
                return Err(CompileError::new(
                    "SEM_015",
                    format!("field access on non-struct type {}", other.name()),
                    Some(span),
                ))
            }
        };
        let def = self.analyzer.symbols.lookup_struct(&struct_name).ok_or_else(|| {
            CompileError::new("SEM_001", format!("unknown struct '{}'", struct_name), Some(span))
        })?;
        let layout = crate::ast::struct_layout(def, &self.analyzer.symbols.structs);
        layout
            .into_iter()
            .find(|f| f.name == field)
            .map(|f| (f.offset, f.ty))
            .ok_or_else(|| {
                CompileError::new(
                    "SEM_015",
                    format!("struct '{}' has no field '{}'", struct_name, field),
                    Some(span),
                )
            })
    }

    fn emit_store(&self, ty: &Type, mem: &str, asm: &mut String) {
        match ty.size_in_bytes(&self.analyzer.symbols.structs) {
            2 => asm.push_str(&format!("    mov {}, ax\n", mem)),
            1 => asm.push_str(&format!("    mov {}, al\n", mem)),
            _ => asm.push_str(&format!("    mov {}, eax\n", mem)),
        }
    }

    fn load_at(&self, ty: &Type, addr: &str, offset: i32, asm: &mut String) {
        let deref = if offset == 0 {
            format!("[{}]", addr)
        } else {
            format!("[{} + {}]", addr, offset)
        };
        match ty.size_in_bytes(&self.analyzer.symbols.structs) {
            2 => asm.push_str(&format!("    movzx eax, word ptr {}\n", deref)),
            1 => asm.push_str(&format!("    movzx eax, byte ptr {}\n", deref)),
            _ => asm.push_str(&format!("    mov eax, {}\n", deref)),
        }
    }

    /// Address of an lvalue expression in eax (struct value, field, index,
    /// dereference, or &x).
    fn generate_addr(&mut self, expr: &Expr, asm: &mut String) -> Result<(), CompileError> {
        match expr {
            Expr::Ident(name, span) => {
                if let Some(sym) = self.lookup_var(name) {
                    asm.push_str(&format!("    lea eax, [ebp + ({})]\n", sym.offset));
                } else if let Some(g) = self.analyzer.symbols.lookup_global(name) {
                    asm.push_str(&format!("    lea eax, [{}]\n", Self::global_label(&g.name)));
                } else {
                    return Err(CompileError::new(
                        "GEN_001",
                        format!("unknown variable '{}'", name),
                        Some(*span),
                    ));
                }
                Ok(())
            }
            Expr::Field(base, field, span) => {
                let ty = self.type_of(base)?;
                let (foff, _) = self.field_info_from_type(&ty, field, *span)?;
                match ty {
                    Type::Struct(_) => self.generate_addr(base, asm)?,
                    Type::Pointer(_) => asm.push_str(&self.generate_expr(base)?),
                    _ => unreachable!(),
                }
                if foff != 0 {
                    asm.push_str(&format!("    add eax, {}\n", foff));
                }
                Ok(())
            }
            Expr::Index(base, idx, span) => {
                let elem_size = match self.type_of(base)? {
                    Type::Array(elem, _) | Type::Pointer(elem) => {
                        elem.size_in_bytes(&self.analyzer.symbols.structs)
                    }
                    other => {
                        return Err(CompileError::new(
                            "GEN_008",
                            format!("cannot index value of type {}", other.name()),
                            Some(*span),
                        ))
                    }
                };
                self.generate_index_addr(base, idx, elem_size, *span, asm)
            }
            Expr::Unary(UnaryOp::Deref, ptr, _) => {
                asm.push_str(&self.generate_expr(ptr)?);
                Ok(())
            }
            Expr::Unary(UnaryOp::AddrOf, inner, _) => self.generate_addr(inner, asm),
            _ => Err(CompileError::new(
                "GEN_004",
                "expression is not addressable",
                Some(expr.span()),
            )),
        }
    }

    fn generate_index_addr(
        &mut self,
        base: &Expr,
        idx: &Expr,
        elem_size: usize,
        span: Span,
        asm: &mut String,
    ) -> Result<(), CompileError> {
        if elem_size != 1 && elem_size != 2 && elem_size != 4 {
            return Err(CompileError::new(
                "GEN_009",
                format!("unsupported element size {} for indexing", elem_size),
                Some(span),
            ));
        }
        asm.push_str(&self.generate_expr(idx)?);
        asm.push_str("    push eax\n");
        match self.type_of(base)? {
            Type::Array(..) => self.generate_addr(base, asm)?,
            Type::Pointer(..) => asm.push_str(&self.generate_expr(base)?),
            other => {
                return Err(CompileError::new(
                    "GEN_008",
                    format!("cannot index value of type {}", other.name()),
                    Some(span),
                ))
            }
        }
        asm.push_str("    pop ecx\n");
        match elem_size {
            2 => asm.push_str("    shl ecx, 1\n"),
            4 => asm.push_str("    shl ecx, 2\n"),
            _ => {}
        }
        asm.push_str("    add eax, ecx\n");
        Ok(())
    }

    fn generate_struct_lit(
        &mut self,
        dest_off: i32,
        struct_name: &str,
        inits: &[(String, Expr)],
        asm: &mut String,
    ) -> Result<(), CompileError> {
        let def = self
            .analyzer
            .symbols
            .lookup_struct(struct_name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new(
                    "SEM_018",
                    format!("unknown struct '{}'", struct_name),
                    None,
                )
            })?;
        let layout = crate::ast::struct_layout(&def, &self.analyzer.symbols.structs);
        for (field_name, value) in inits {
            let (foff, fty) = layout
                .iter()
                .find(|f| f.name == *field_name)
                .map(|f| (f.offset as i32, f.ty.clone()))
                .ok_or_else(|| {
                    CompileError::new(
                        "SEM_015",
                        format!("struct '{}' has no field '{}'", struct_name, field_name),
                        None,
                    )
                })?;
            self.emit_field_store(dest_off, foff, &fty, value, asm)?;
        }
        Ok(())
    }

    /// Writes one field/element value to [ebp + (dest_off + foff)], recursing
    /// for nested aggregate literals.
    fn emit_field_store(
        &mut self,
        dest_off: i32,
        foff: i32,
        ty: &Type,
        value: &Expr,
        asm: &mut String,
    ) -> Result<(), CompileError> {
        match value {
            Expr::StructLit(name, inits, _) => {
                self.generate_struct_lit(dest_off + foff, name, inits, asm)
            }
            Expr::ArrayLit(elems, _) => {
                let elem_ty = match ty {
                    Type::Array(elem, _) => (**elem).clone(),
                    _ => self.type_of(&elems[0])?,
                };
                self.generate_array_lit(dest_off + foff, &elem_ty, elems, asm)
            }
            _ => {
                asm.push_str(&self.generate_expr(value)?);
                self.emit_store(ty, &format!("[ebp + ({})]", dest_off + foff), asm);
                Ok(())
            }
        }
    }

    fn generate_array_lit(
        &mut self,
        dest_off: i32,
        elem_ty: &Type,
        elems: &[Expr],
        asm: &mut String,
    ) -> Result<(), CompileError> {
        let elem_size = elem_ty.size_in_bytes(&self.analyzer.symbols.structs);
        for (i, elem) in elems.iter().enumerate() {
            asm.push_str(&self.generate_expr(elem)?);
            self.emit_store(
                elem_ty,
                &format!("[ebp + ({})]", dest_off + (i * elem_size) as i32),
                asm,
            );
        }
        Ok(())
    }

    fn generate_aggregate_assign(
        &mut self,
        target: &Expr,
        value: &Expr,
        asm: &mut String,
    ) -> Result<(), CompileError> {
        self.generate_addr(target, asm)?;
        asm.push_str("    push eax\n");
        match value {
            Expr::StructLit(name, inits, _) => {
                self.write_struct_lit_dynamic(name, inits, asm)?
            }
            Expr::ArrayLit(elems, _) => {
                let elem_ty = match self.type_of(target)? {
                    Type::Array(elem, _) => *elem,
                    other => {
                        return Err(CompileError::new(
                            "GEN_008",
                            format!("cannot assign array to {}", other.name()),
                            None,
                        ))
                    }
                };
                self.write_array_lit_dynamic(&elem_ty, elems, asm)?
            }
            _ => unreachable!(),
        }
        asm.push_str("    add esp, 4\n");
        Ok(())
    }

    /// Writes a struct literal into the address held at [esp] (kept there for
    /// the whole loop so value expressions may use the stack freely).
    fn write_struct_lit_dynamic(
        &mut self,
        struct_name: &str,
        inits: &[(String, Expr)],
        asm: &mut String,
    ) -> Result<(), CompileError> {
        let def = self
            .analyzer
            .symbols
            .lookup_struct(struct_name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new(
                    "SEM_018",
                    format!("unknown struct '{}'", struct_name),
                    None,
                )
            })?;
        let layout = crate::ast::struct_layout(&def, &self.analyzer.symbols.structs);
        for (field_name, init) in inits {
            let (foff, fty) = layout
                .iter()
                .find(|f| f.name == *field_name)
                .map(|f| (f.offset as i32, f.ty.clone()))
                .ok_or_else(|| {
                    CompileError::new(
                        "SEM_015",
                        format!("struct '{}' has no field '{}'", struct_name, field_name),
                        None,
                    )
                })?;
            match init {
                Expr::StructLit(name, inits, _) => {
                    asm.push_str(&format!("    mov eax, [esp]\n    add eax, {}\n    push eax\n", foff));
                    self.write_struct_lit_dynamic(name, inits, asm)?;
                    asm.push_str("    add esp, 4\n");
                }
                Expr::ArrayLit(elems, _) => {
                    let elem_ty = match &fty {
                        Type::Array(elem, _) => (**elem).clone(),
                        _ => self.type_of(&elems[0])?,
                    };
                    asm.push_str(&format!("    mov eax, [esp]\n    add eax, {}\n    push eax\n", foff));
                    self.write_array_lit_dynamic(&elem_ty, elems, asm)?;
                    asm.push_str("    add esp, 4\n");
                }
                _ => {
                    asm.push_str(&self.generate_expr(init)?);
                    asm.push_str("    mov edx, [esp]\n");
                    self.emit_store_to_edx(&fty, foff, asm);
                }
            }
        }
        Ok(())
    }

    fn write_array_lit_dynamic(
        &mut self,
        elem_ty: &Type,
        elems: &[Expr],
        asm: &mut String,
    ) -> Result<(), CompileError> {
        let elem_size = elem_ty.size_in_bytes(&self.analyzer.symbols.structs);
        for (i, elem) in elems.iter().enumerate() {
            asm.push_str(&self.generate_expr(elem)?);
            asm.push_str("    mov edx, [esp]\n");
            self.emit_store_to_edx(elem_ty, (i * elem_size) as i32, asm);
        }
        Ok(())
    }

    /// Address of the storage backing an expression value: lvalues use their
    /// address, anything else must already evaluate to a pointer.
    fn addr_of_value(&mut self, value: &Expr, asm: &mut String) -> Result<(), CompileError> {
        match value {
            Expr::Ident(..) | Expr::Field(..) | Expr::Index(..) => {
                self.generate_addr(value, asm)?;
            }
            _ => asm.push_str(&self.generate_expr(value)?),
        }
        Ok(())
    }

    /// Copies `size` bytes from ebx (source) to eax (destination), byte by byte.
    fn emit_copy_loop(&mut self, asm: &mut String, size: usize) {
        let loop_lbl = self.new_label("copy");
        let done_lbl = self.new_label("copy_done");
        asm.push_str(&format!(
            "    mov ecx, {}\n{}:\n    mov dl, [ebx]\n    mov [eax], dl\n    inc ebx\n    inc eax\n    dec ecx\n    jnz {}\n{}:\n",
            size, loop_lbl, loop_lbl, done_lbl
        ));
    }

    fn copy_into_slot(
        &mut self,
        dest: &str,
        src: &Expr,
        asm: &mut String,
    ) -> Result<(), CompileError> {
        asm.push_str(&format!("    lea eax, {}\n", dest));
        asm.push_str("    push eax\n");
        self.addr_of_value(src, asm)?;
        asm.push_str("    mov ebx, eax\n    pop eax\n");
        let size = self.type_of(src)?.size_in_bytes(&self.analyzer.symbols.structs);
        self.emit_copy_loop(asm, size);
        Ok(())
    }

    fn generate_global_decl(&mut self, stmt: &Stmt) -> Result<String, CompileError> {
        let (name, span) = match stmt {
            Stmt::Let { name, span, .. } => (name.clone(), *span),
            _ => unreachable!("global item is always a let"),
        };
        let g = self
            .analyzer
            .symbols
            .lookup_global(&name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new("GEN_001", format!("unknown global '{}'", name), Some(span))
            })?;
        let mut out = String::new();
        out.push_str(&format!("{}:\n", Self::global_label(&name)));
        let mut cursor = 0u32;
        self.emit_global_value(&g.ty, g.init.as_ref(), &mut cursor, &mut out)?;
        Ok(out)
    }

    fn emit_global_value(
        &mut self,
        ty: &Type,
        value: Option<&crate::sema::symbols::ConstValue>,
        cursor: &mut u32,
        out: &mut String,
    ) -> Result<(), CompileError> {
        use crate::sema::symbols::ConstValue;
        let size = ty.size_in_bytes(&self.analyzer.symbols.structs);
        let align = (size as u32).min(4);
        let pad = (*cursor % align) as usize;
        if pad != 0 {
            out.push_str(&format!("    .space {}\n", align as usize - pad));
            *cursor += (align as usize - pad) as u32;
        }

        match ty {
            Type::Array(elem, len) => {
                let values = match value {
                    Some(ConstValue::Array(values)) => values,
                    _ => {
                        out.push_str(&format!("    .space {}\n", size));
                        *cursor += size as u32;
                        return Ok(());
                    }
                };
                for i in 0..*len {
                    self.emit_global_value(elem, values.get(i), cursor, out)?;
                }
            }
            Type::Struct(name) => {
                let def = self
                    .analyzer
                    .symbols
                    .lookup_struct(name)
                    .cloned()
                    .ok_or_else(|| {
                        CompileError::new("SEM_001", format!("unknown struct '{}'", name), None)
                    })?;
                let fields = match value {
                    Some(ConstValue::Struct(fields)) => fields.clone(),
                    _ => Vec::new(),
                };
                for field in &def.fields {
                    let field_value = fields
                        .iter()
                        .find(|(fname, _)| fname == &field.name)
                        .map(|(_, v)| v);
                    self.emit_global_value(&field.ty, field_value, cursor, out)?;
                }
            }
            scalar => {
                let width = scalar.size_in_bytes(&self.analyzer.symbols.structs);
                match value {
                    Some(ConstValue::Int(n)) => {
                        if !crate::sema::types::int_fits(scalar, *n) {
                            return Err(CompileError::new(
                                "GEN_011",
                                format!("value {} does not fit in {}", n, scalar.name()),
                                None,
                            ));
                        }
                        match width {
                            1 => out.push_str(&format!("    .byte {}\n", n)),
                            2 => out.push_str(&format!("    .word {}\n", n)),
                            _ => out.push_str(&format!("    .long {}\n", n)),
                        }
                    }
                    Some(ConstValue::Bool(b)) => {
                        out.push_str(&format!("    .byte {}\n", if *b { 1 } else { 0 }))
                    }
                    Some(ConstValue::Str(s)) => {
                        let id = self.add_string_literal(s);
                        out.push_str(&format!("    .long .LC{}\n", id));
                    }
                    _ => out.push_str(&format!("    .space {}\n", width)),
                }
                *cursor += width as u32;
            }
        }
        Ok(())
    }

    fn copy_aggregate_assign(
        &mut self,
        target: &Expr,
        value: &Expr,
        asm: &mut String,
    ) -> Result<(), CompileError> {
        if let Expr::Ident(name, _) = target {
            let dest = if let Some(sym) = self.lookup_var(name) {
                Some(format!("[ebp + ({})]", sym.offset))
            } else {
                self.analyzer
                    .symbols
                    .lookup_global(name)
                    .map(|g| format!("[{}]", Self::global_label(&g.name)))
            };
            if let Some(dest) = dest {
                return self.copy_into_slot(&dest, value, asm);
            }
        }
        self.generate_addr(target, asm)?;
        asm.push_str("    push eax\n");
        self.addr_of_value(value, asm)?;
        asm.push_str("    mov ebx, eax\n    pop eax\n");
        let size = self
            .type_of(target)?
            .size_in_bytes(&self.analyzer.symbols.structs);
        self.emit_copy_loop(asm, size);
        Ok(())
    }

    fn emit_store_to_edx(&self, ty: &Type, offset: i32, asm: &mut String) {
        let deref = if offset == 0 {
            "[edx]".to_string()
        } else {
            format!("[edx + {}]", offset)
        };
        self.emit_store(ty, &deref, asm);
    }
}
