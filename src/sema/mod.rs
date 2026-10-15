pub mod symbols;
pub mod types;

use crate::ast::*;
use crate::error::CompileError;
use crate::token::Span;
use self::symbols::{FnSymbol, Symbol, SymbolTable};
use self::types::{int_fits, is_assignable};

pub struct SemanticAnalyzer {
    pub symbols: SymbolTable,
    pub has_sys_import: bool,
    loop_depth: usize,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut symbols = SymbolTable::new();

        // Built-in NyxaraOS syscall helpers
        symbols.register_fn(FnSymbol {
            name: "sys::exit".into(),
            param_types: vec![Type::I32],
            ret_type: Type::Void,
            is_extern: true,
        });
        symbols.register_fn(FnSymbol {
            name: "sys::write".into(),
            param_types: vec![Type::I32, Type::Pointer(Box::new(Type::U8)), Type::I32],
            ret_type: Type::I32,
            is_extern: true,
        });
        symbols.register_fn(FnSymbol {
            name: "sys::read".into(),
            param_types: vec![Type::I32, Type::Pointer(Box::new(Type::U8)), Type::I32],
            ret_type: Type::I32,
            is_extern: true,
        });
        symbols.register_fn(FnSymbol {
            name: "sys::fork".into(),
            param_types: vec![],
            ret_type: Type::I32,
            is_extern: true,
        });
        symbols.register_fn(FnSymbol {
            name: "sys::waitpid".into(),
            param_types: vec![Type::I32, Type::Pointer(Box::new(Type::I32)), Type::I32],
            ret_type: Type::I32,
            is_extern: true,
        });
        symbols.register_fn(FnSymbol {
            name: "sys::execve".into(),
            param_types: vec![
                Type::Pointer(Box::new(Type::U8)),
                Type::Pointer(Box::new(Type::Pointer(Box::new(Type::U8)))),
                Type::Pointer(Box::new(Type::Pointer(Box::new(Type::U8)))),
            ],
            ret_type: Type::I32,
            is_extern: true,
        });
        symbols.register_fn(FnSymbol {
            name: "sys::getpid".into(),
            param_types: vec![],
            ret_type: Type::I32,
            is_extern: true,
        });

        // Compiler built-ins, inlined by codegen
        symbols.register_fn(FnSymbol {
            name: "str_len".into(),
            param_types: vec![Type::Pointer(Box::new(Type::U8))],
            ret_type: Type::I32,
            is_extern: false,
        });
        symbols.register_fn(FnSymbol {
            name: "itoa".into(),
            param_types: vec![Type::I32],
            ret_type: Type::Pointer(Box::new(Type::U8)),
            is_extern: false,
        });
        symbols.register_fn(FnSymbol {
            name: "print".into(),
            param_types: vec![Type::Pointer(Box::new(Type::U8))],
            ret_type: Type::Void,
            is_extern: false,
        });
        symbols.register_fn(FnSymbol {
            name: "println".into(),
            param_types: vec![Type::Pointer(Box::new(Type::U8))],
            ret_type: Type::Void,
            is_extern: false,
        });

        Self {
            symbols,
            has_sys_import: false,
            loop_depth: 0,
        }
    }

    pub fn analyze_program(&mut self, program: &mut Program) -> Result<(), CompileError> {
        // First pass: register imports, function signatures, and top-level constants
        for item in &program.items {
            match item {
                Item::Import(imp) => {
                    if imp.path == "nyx/sys" {
                        self.has_sys_import = true;
                    }
                }
                Item::Fn(f) => {
                    let fn_sym = FnSymbol {
                        name: f.name.clone(),
                        param_types: f.params.iter().map(|p| p.ty.clone()).collect(),
                        ret_type: f.ret_ty.clone(),
                        is_extern: f.is_extern,
                    };
                    self.symbols.register_fn(fn_sym);
                }
                Item::Struct(def) => {
                    if self.symbols.lookup_struct(&def.name).is_some() {
                        return Err(CompileError::new(
                            "SEM_019",
                            format!("duplicate struct definition '{}'", def.name),
                            Some(def.span),
                        ));
                    }
                    self.symbols.register_struct(def.clone());
                }
                Item::Global(Stmt::Let {
                    name,
                    is_mut,
                    ty,
                    init,
                    span,
                }) => {
                    self.analyze_global(name, *is_mut, ty.as_ref(), init.as_ref(), *span)?;
                }
                Item::Global(_) => {}
                Item::Const(Stmt::Const { name, ty, init, span }) => {
                    self.analyze_const(name, ty.as_ref(), init, *span)?;
                }
                Item::Const(_) => {}
            }
        }

        // Second pass: analyze function bodies (collect fn data first to avoid borrow conflicts)
        let fn_bodies: Vec<(usize, Vec<Stmt>)> = program
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if let Item::Fn(f) = item {
                    f.body.as_ref().map(|body| (i, body.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (idx, body) in fn_bodies {
            if let Item::Fn(ref mut f) = program.items[idx] {
                self.analyze_fn(f, &body)?;
            }
        }

        Ok(())
    }

    fn analyze_const(
        &mut self,
        name: &str,
        ty: Option<&Type>,
        init: &Expr,
        span: Span,
    ) -> Result<(), CompileError> {
        let (val_ty, const_val) = self.eval_const_expr(init)?;
        let final_ty = if let Some(expected_ty) = ty {
            if !is_assignable(expected_ty, &val_ty) {
                return Err(CompileError::new(
                    "SEM_003",
                    format!(
                        "type mismatch: cannot assign {} to {}",
                        val_ty.name(),
                        expected_ty.name()
                    ),
                    Some(span),
                ));
            }
            expected_ty.clone()
        } else {
            val_ty
        };

        self.symbols.register_const(crate::sema::symbols::ConstSymbol {
            name: name.to_string(),
            ty: final_ty,
            value: const_val,
        });
        Ok(())
    }

    pub fn eval_const_expr(
        &self,
        expr: &Expr,
    ) -> Result<(Type, crate::sema::symbols::ConstValue), CompileError> {
        use crate::sema::symbols::ConstValue;
        match expr {
            Expr::IntLit(n, _) => Ok((Type::I32, ConstValue::Int(*n))),
            Expr::BoolLit(b, _) => Ok((Type::Bool, ConstValue::Bool(*b))),
            Expr::StringLit(s, _) => {
                Ok((Type::Pointer(Box::new(Type::U8)), ConstValue::Str(s.clone())))
            }
            Expr::Null(_) => Ok((Type::Pointer(Box::new(Type::U8)), ConstValue::Int(0))),
            Expr::Sizeof(ty, _) => Ok((
                Type::I32,
                ConstValue::Int(ty.size_in_bytes(&self.symbols.structs) as i64),
            )),
            Expr::Cast(inner, _, _) => {
                let (_, value) = self.eval_const_expr(inner)?;
                Ok((Type::I32, value))
            }
            Expr::Ident(name, span) => {
                if let Some(cs) = self.symbols.lookup_const(name) {
                    Ok((cs.ty.clone(), cs.value.clone()))
                } else {
                    Err(CompileError::new(
                        "SEM_001",
                        format!("cannot find constant '{}' in this scope", name),
                        Some(*span),
                    ))
                }
            }
            Expr::Unary(UnaryOp::Neg, inner, span) => {
                let (ty, val) = self.eval_const_expr(inner)?;
                match val {
                    ConstValue::Int(n) => Ok((ty, ConstValue::Int(-n))),
                    _ => Err(CompileError::new(
                        "SEM_003",
                        "cannot negate non-integer constant",
                        Some(*span),
                    )),
                }
            }
            Expr::Unary(UnaryOp::Not, inner, span) => {
                let (ty, val) = self.eval_const_expr(inner)?;
                match val {
                    ConstValue::Bool(b) => Ok((ty, ConstValue::Bool(!b))),
                    _ => Err(CompileError::new(
                        "SEM_003",
                        "cannot invert non-boolean constant",
                        Some(*span),
                    )),
                }
            }
            Expr::Unary(UnaryOp::BitNot, inner, span) => {
                let (ty, val) = self.eval_const_expr(inner)?;
                match val {
                    ConstValue::Int(n) => Ok((ty, ConstValue::Int(!n))),
                    _ => Err(CompileError::new(
                        "SEM_003",
                        "cannot bitwise invert non-integer constant",
                        Some(*span),
                    )),
                }
            }
            Expr::Binary(op, left, right, span) => {
                let (_, lval) = self.eval_const_expr(left)?;
                let (_, rval) = self.eval_const_expr(right)?;
                match (lval, rval) {
                    (ConstValue::Int(l), ConstValue::Int(r)) => {
                        let res = match op {
                            BinaryOp::Add => ConstValue::Int(l + r),
                            BinaryOp::Sub => ConstValue::Int(l - r),
                            BinaryOp::Mul => ConstValue::Int(l * r),
                            BinaryOp::Div => {
                                if r == 0 {
                                    return Err(CompileError::new(
                                        "SEM_014",
                                        "division by zero in constant",
                                        Some(*span),
                                    ));
                                }
                                ConstValue::Int(l / r)
                            }
                            BinaryOp::Mod => {
                                if r == 0 {
                                    return Err(CompileError::new(
                                        "SEM_014",
                                        "division by zero in constant",
                                        Some(*span),
                                    ));
                                }
                                ConstValue::Int(l % r)
                            }
                            BinaryOp::BitAnd => ConstValue::Int(l & r),
                            BinaryOp::BitOr => ConstValue::Int(l | r),
                            BinaryOp::BitXor => ConstValue::Int(l ^ r),
                            BinaryOp::Shl => ConstValue::Int(l << r),
                            BinaryOp::Shr => ConstValue::Int(l >> r),
                            BinaryOp::Eq => return Ok((Type::Bool, ConstValue::Bool(l == r))),
                            BinaryOp::Ne => return Ok((Type::Bool, ConstValue::Bool(l != r))),
                            BinaryOp::Lt => return Ok((Type::Bool, ConstValue::Bool(l < r))),
                            BinaryOp::Le => return Ok((Type::Bool, ConstValue::Bool(l <= r))),
                            BinaryOp::Gt => return Ok((Type::Bool, ConstValue::Bool(l > r))),
                            BinaryOp::Ge => return Ok((Type::Bool, ConstValue::Bool(l >= r))),
                            _ => {
                                return Err(CompileError::new(
                                    "SEM_003",
                                    "unsupported binary op on constants",
                                    Some(*span),
                                ))
                            }
                        };
                        Ok((Type::I32, res))
                    }
                    (ConstValue::Bool(l), ConstValue::Bool(r)) => {
                        let res = match op {
                            BinaryOp::And => ConstValue::Bool(l && r),
                            BinaryOp::Or => ConstValue::Bool(l || r),
                            BinaryOp::Eq => ConstValue::Bool(l == r),
                            BinaryOp::Ne => ConstValue::Bool(l != r),
                            _ => {
                                return Err(CompileError::new(
                                    "SEM_003",
                                    "unsupported boolean op on constants",
                                    Some(*span),
                                ))
                            }
                        };
                        Ok((Type::Bool, res))
                    }
                    _ => Err(CompileError::new(
                        "SEM_003",
                        "type mismatch in constant expression",
                        Some(*span),
                    )),
                }
            }
            Expr::ArrayLit(elems, _span) => {
                let mut values = Vec::with_capacity(elems.len());
                let mut elem_ty = Type::I32;
                for (i, elem) in elems.iter().enumerate() {
                    let (ty, value) = self.eval_const_expr(elem)?;
                    if i == 0 {
                        elem_ty = ty;
                    }
                    values.push(value);
                }
                Ok((
                    Type::Array(Box::new(elem_ty), elems.len()),
                    ConstValue::Array(values),
                ))
            }
            Expr::StructLit(name, inits, span) => {
                let def = self
                    .symbols
                    .lookup_struct(name)
                    .ok_or_else(|| {
                        CompileError::new(
                            "SEM_018",
                            format!("unknown struct '{}' in literal", name),
                            Some(*span),
                        )
                    })?
                    .clone();
                let mut fields = Vec::with_capacity(inits.len());
                for (field_name, value) in inits {
                    if !def.fields.iter().any(|f| f.name == *field_name) {
                        return Err(CompileError::new(
                            "SEM_017",
                            format!("struct '{}' has no field '{}'", name, field_name),
                            Some(*span),
                        ));
                    }
                    fields.push((field_name.clone(), self.eval_const_expr(value)?.1));
                }
                Ok((Type::Struct(name.clone()), ConstValue::Struct(fields)))
            }
            _ => Err(CompileError::new(
                "SEM_003",
                "expression is not a valid constant",
                Some(expr.span()),
            )),
        }
    }

    /// Assignment/initializer compatibility: type rules plus integer literals
    /// that fit a narrower integer target (e.g. an i16 struct field).
    fn check_assign_value(&self, target: &Type, value: &Expr, span: Span) -> Result<(), CompileError> {
        if let Expr::IntLit(n, _) = value {
            if int_fits(target, *n) {
                return Ok(());
            }
        }
        if let Expr::Unary(UnaryOp::Neg, box_expr, _) = value {
            if let Expr::IntLit(n, _) = box_expr.as_ref() {
                if int_fits(target, -*n) {
                    return Ok(());
                }
            }
        }
        if let (Type::Array(elem, len), Expr::ArrayLit(elems, _)) = (target, value) {
            if *len == elems.len()
                && elems
                    .iter()
                    .all(|e| matches!(e, Expr::IntLit(n, _) if int_fits(elem, *n)))
            {
                return Ok(());
            }
        }
        let val_ty = self.type_of_expr(value)?;
        if is_assignable(target, &val_ty) {
            Ok(())
        } else {
            Err(CompileError::new(
                "SEM_003",
                format!(
                    "type mismatch: cannot assign {} to {}",
                    val_ty.name(),
                    target.name()
                ),
                Some(span),
            ))
        }
    }

    /// Walks an assignment target (variable, field, index, deref) and rejects
    /// writes to constants and immutable variables at the root.
    fn analyze_global(
        &mut self,
        name: &str,
        is_mut: bool,
        ty: Option<&Type>,
        init: Option<&Expr>,
        span: Span,
    ) -> Result<(), CompileError> {
        if self.symbols.lookup_global(name).is_some() {
            return Err(CompileError::new(
                "SEM_021",
                format!("duplicate global '{}'", name),
                Some(span),
            ));
        }

        let value = match init {
            Some(expr) => Some(self.eval_const_expr(expr)?.1),
            None => None,
        };
        let final_ty = match (ty, init) {
            (Some(explicit_ty), _) => explicit_ty.clone(),
            (None, Some(expr)) => self.type_of_expr(expr)?,
            (None, None) => {
                return Err(CompileError::new(
                    "SEM_022",
                    format!("global '{}' needs a type annotation or an initializer", name),
                    Some(span),
                ))
            }
        };

        self.symbols.register_global(crate::sema::symbols::GlobalSymbol {
            name: name.to_string(),
            ty: final_ty,
            is_mut,
            init: value,
        });
        Ok(())
    }

    fn check_mut_target(&self, target: &Expr) -> Result<(), CompileError> {
        match target {
            Expr::Ident(name, span) => {
                if self.symbols.lookup_const(name).is_some() {
                    return Err(CompileError::new(
                        "SEM_004",
                        format!("cannot assign to constant '{}'", name),
                        Some(*span),
                    ));
                }
                if let Some(sym) = self.symbols.lookup_var(name) {
                    if !sym.is_mut {
                        return Err(CompileError::new(
                            "SEM_004",
                            format!("cannot assign twice to immutable variable '{}'", name),
                            Some(*span),
                        ));
                    }
                }
                if let Some(g) = self.symbols.lookup_global(name) {
                    if !g.is_mut {
                        return Err(CompileError::new(
                            "SEM_004",
                            format!("cannot assign to immutable global '{}'", name),
                            Some(*span),
                        ));
                    }
                }
                Ok(())
            }
            Expr::Field(base, _, _) | Expr::Index(base, _, _) => self.check_mut_target(base),
            Expr::Unary(UnaryOp::Deref, _, _) => Ok(()),
            _ => Ok(()),
        }
    }

    fn analyze_fn(&mut self, def: &mut FnDef, body: &[Stmt]) -> Result<(), CompileError> {
        self.symbols.enter_scope();

        // Register parameters with positive offsets: EBP + 8, EBP + 12, etc.
        let mut param_offset = 8;
        for param in &def.params {
            self.symbols.record_fn_var(&def.name, Symbol {
                name: param.name.clone(),
                ty: param.ty.clone(),
                is_mut: true,
                offset: param_offset,
            });
            param_offset += 4;
        }

        // Track local variable offsets (negative from EBP)
        let mut local_offset = -4;
        let mut local_offset_min = local_offset;
        for stmt in body {
            self.analyze_stmt(stmt, &def.name, &def.ret_ty, &mut local_offset)?;
            if local_offset < local_offset_min {
                local_offset_min = local_offset;
            }
        }

        self.symbols.exit_scope();

        // Compute stack space needed for locals (0 when no locals declared).
        if local_offset_min < -4 {
            def.stack_size = (-local_offset_min) - 4;
        } else {
            def.stack_size = 0;
        }

        Ok(())
    }
    fn is_castable(source: &Type, target: &Type) -> bool {
        if source == target {
            return true;
        }
        let source_integral = matches!(
            source,
            Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32 | Type::Bool
        );
        let target_integral = matches!(
            target,
            Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32 | Type::Bool
        );
        if source_integral && target_integral {
            return true;
        }
        if let (Type::Pointer(_), Type::Pointer(_)) = (source, target) {
            return true;
        }
        match (source, target) {
            (Type::Pointer(_), Type::I32) | (Type::Pointer(_), Type::U32)
            | (Type::I32, Type::Pointer(_)) | (Type::U32, Type::Pointer(_))
            | (Type::Pointer(_), Type::Bool) | (Type::Bool, Type::Pointer(_)) => {
                true
            }
            _ => false,
        }
    }


    fn analyze_stmt(
        &mut self,
        stmt: &Stmt,
        fn_name: &str,
        expected_ret: &Type,
        local_offset: &mut i32,
    ) -> Result<(), CompileError> {
        match stmt {
            Stmt::Let { name, is_mut, ty, init, span } => {
                let init_ty = if let Some(expr) = init {
                    Some(self.type_of_expr(expr)?)
                } else {
                    None
                };

                let var_ty = match (ty, &init_ty) {
                    (Some(explicit_ty), Some(_)) => {
                        self.check_assign_value(explicit_ty, init.as_ref().unwrap(), *span)?;
                        explicit_ty.clone()
                    }
                    (Some(explicit_ty), None) => explicit_ty.clone(),
                    (None, Some(expr_ty)) => expr_ty.clone(),
                    (None, None) => {
                        return Err(CompileError::new(
                            "SEM_009",
                            format!(
                                "variable '{}' needs an explicit type or an initializer",
                                name
                            ),
                            Some(*span),
                        ));
                    }
                };

                let size = var_ty.size_in_bytes(&self.symbols.structs);
                let alloc = if size < 4 { 4 } else { ((size + 3) & !3) as i32 };
                self.symbols.record_fn_var(fn_name, Symbol {
                    name: name.clone(),
                    ty: var_ty,
                    is_mut: *is_mut,
                    offset: *local_offset,
                });
                *local_offset -= alloc;
            }
            Stmt::Return { value, span } => {
                let val_ty = if let Some(expr) = value {
                    self.type_of_expr(expr)?
                } else {
                    Type::Void
                };

                if !is_assignable(expected_ret, &val_ty) {
                    return Err(CompileError::new(
                        "SEM_005",
                        format!(
                            "invalid return type: expected {}, got {}",
                            expected_ret.name(),
                            val_ty.name()
                        ),
                        Some(*span),
                    ));
                }
            }
            Stmt::Expr(expr, _) => {
                self.type_of_expr(expr)?;
            }
            Stmt::If { cond, then_branch, else_branch, span } => {
                let cond_ty = self.type_of_expr(cond)?;
                if !matches!(cond_ty, Type::Bool | Type::I32 | Type::U32 | Type::Pointer(_)) {
                    return Err(CompileError::new(
                        "SEM_008",
                        format!(
                            "if condition must be a bool, integer, or pointer, found {}",
                            cond_ty.name()
                        ),
                        Some(*span),
                    ));
                }
                for s in then_branch {
                    self.analyze_stmt(s, fn_name, expected_ret, local_offset)?;
                }
                if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        self.analyze_stmt(s, fn_name, expected_ret, local_offset)?;
                    }
                }
            }
            Stmt::While { cond, body, span } => {
                let cond_ty = self.type_of_expr(cond)?;
                if !matches!(cond_ty, Type::Bool | Type::I32 | Type::U32 | Type::Pointer(_)) {
                    return Err(CompileError::new(
                        "SEM_008",
                        format!(
                            "while condition must be a bool, integer, or pointer, found {}",
                            cond_ty.name()
                        ),
                        Some(*span),
                    ));
                }
                self.loop_depth += 1;
                for s in body {
                    self.analyze_stmt(s, fn_name, expected_ret, local_offset)?;
                }
                self.loop_depth -= 1;
            }
            Stmt::Break { span } | Stmt::Continue { span } => {
                if self.loop_depth == 0 {
                    return Err(CompileError::new(
                        "SEM_020",
                        "break/continue outside of a loop",
                        Some(*span),
                    ));
                }
            }
            Stmt::Block(stmts, _) => {
                self.symbols.enter_scope();
                for s in stmts {
                    self.analyze_stmt(s, fn_name, expected_ret, local_offset)?;
                }
                self.symbols.exit_scope();
            }
            Stmt::For { init, cond, step, body, span } => {
                self.loop_depth += 1;
                let result = self.analyze_for(
                    init,
                    cond,
                    step,
                    body,
                    *span,
                    fn_name,
                    expected_ret,
                    local_offset,
                );
                self.loop_depth -= 1;
                result?;
            }
            Stmt::LetWrapper(inner) => {
                let _ = self.analyze_stmt(inner, fn_name, expected_ret, local_offset)?;
            }
            Stmt::ExprInit(e, _) => {
                let _ = self.type_of_expr(e)?;
            }
            Stmt::Const { name, ty, init, span } => {
                self.analyze_const(name, ty.as_ref(), init, *span)?;
            }
        }
        Ok(())
    }

    fn analyze_for(
        &mut self,
        init: &Stmt,
        cond: &Expr,
        step: &Expr,
        body: &[Stmt],
        span: Span,
        fn_name: &str,
        expected_ret: &Type,
        local_offset: &mut i32,
    ) -> Result<(), CompileError> {
        self.symbols.enter_scope();
        // 1. Analyze init
        match init {
            Stmt::LetWrapper(inner) => {
                self.analyze_stmt(inner, fn_name, expected_ret, local_offset)?;
            }
            Stmt::ExprInit(e, _) => {
                let _ = self.type_of_expr(e)?;
            }
            _ => {}
        }
        // 2. Analyze condition
        let cond_ty = self.type_of_expr(cond)?;
        if !matches!(cond_ty, Type::Bool | Type::I32 | Type::U32 | Type::Pointer(_)) {
            self.symbols.exit_scope();
            return Err(CompileError::new(
                "SEM_008",
                format!(
                    "for condition must be a bool, integer, or pointer, found {}",
                    cond_ty.name()
                ),
                Some(span),
            ));
        }
        // 3. Analyze step expression
        if let Err(e) = self.type_of_expr(step) {
            self.symbols.exit_scope();
            return Err(e);
        }
        // 4. Analyze body statements
        for s in body {
            if let Err(e) = self.analyze_stmt(s, fn_name, expected_ret, local_offset) {
                self.symbols.exit_scope();
                return Err(e);
            }
        }
        self.symbols.exit_scope();
        Ok(())
    }

    pub fn type_of_expr(&self, expr: &Expr) -> Result<Type, CompileError> {
        match expr {
            Expr::IntLit(_, _) => Ok(Type::I32),
            Expr::StringLit(_, _) => Ok(Type::Pointer(Box::new(Type::U8))),
            Expr::BoolLit(_, _) => Ok(Type::Bool),
            Expr::Null(_) => Ok(Type::Pointer(Box::new(Type::U8))),
            Expr::Sizeof(ty, _) => {
                let _ = ty;
                Ok(Type::I32)
            }
            Expr::Cast(expr, target, span) => {
                let source = self.type_of_expr(expr)?;
                if Self::is_castable(&source, target) {
                    Ok(target.clone())
                } else {
                    Err(CompileError::new(
                        "SEM_023",
                        format!(
                            "cannot cast from {} to {}",
                            source.name(),
                            target.name()
                        ),
                        Some(*span),
                    ))
                }
            }
            Expr::Ident(name, span) => {
                if let Some(sym) = self.symbols.lookup_var(name) {
                    Ok(sym.ty.clone())
                } else if let Some(cs) = self.symbols.lookup_const(name) {
                    Ok(cs.ty.clone())
                } else if let Some(g) = self.symbols.lookup_global(name) {
                    Ok(g.ty.clone())
                } else {
                    Err(CompileError::new(
                        "SEM_001",
                        format!("cannot find value '{}' in this scope", name),
                        Some(*span),
                    ))
                }
            }
            Expr::Path(parts, span) => {
                let full_name = parts.join("::");
                if let Some(fn_sym) = self.symbols.lookup_fn(&full_name) {
                    Ok(fn_sym.ret_type.clone())
                } else {
                    Err(CompileError::new(
                        "SEM_002",
                        format!("cannot find symbol '{}' in this scope", full_name),
                        Some(*span),
                    ))
                }
            }
            Expr::Field(base, field, span) => {
                let base_ty = self.type_of_expr(base)?;
                let struct_name = match &base_ty {
                    Type::Struct(name) => name.clone(),
                    Type::Pointer(inner) => match inner.as_ref() {
                        Type::Struct(name) => name.clone(),
                        _ => unreachable!(),
                    },
                    other => {
                        return Err(CompileError::new(
                            "SEM_015",
                            format!("field access on non-struct type {}", other.name()),
                            Some(*span),
                        ))
                    }
                };
                let def = self
                    .symbols
                    .lookup_struct(&struct_name)
                    .ok_or_else(|| {
                        CompileError::new(
                            "SEM_001",
                            format!("unknown struct '{}'", struct_name),
                            Some(*span),
                        )
                    })?;
                def.fields
                    .iter()
                    .find(|f| f.name == *field)
                    .map(|f| f.ty.clone())
                    .ok_or_else(|| {
                        CompileError::new(
                            "SEM_015",
                            format!("struct '{}' has no field '{}'", struct_name, field),
                            Some(*span),
                        )
                    })
            }
            Expr::Index(base, idx, span) => {
                let idx_ty = self.type_of_expr(idx)?;
                if !matches!(
                    idx_ty,
                    Type::I8 | Type::I16 | Type::I32 | Type::U8 | Type::U16 | Type::U32
                ) {
                    return Err(CompileError::new(
                        "SEM_016",
                        format!("array index must be an integer, found {}", idx_ty.name()),
                        Some(*span),
                    ));
                }
                match self.type_of_expr(base)? {
                    Type::Array(elem, _) => Ok(*elem),
                    Type::Pointer(elem) => Ok(*elem),
                    other => Err(CompileError::new(
                        "SEM_016",
                        format!("cannot index value of type {}", other.name()),
                        Some(*span),
                    )),
                }
            }
            Expr::StructLit(name, inits, span) => {
                let def = self.symbols.lookup_struct(name).ok_or_else(|| {
                    CompileError::new(
                        "SEM_018",
                        format!("unknown struct '{}' in literal", name),
                        Some(*span),
                    )
                })?;
                for (field_name, value) in inits {
                    let field_ty = def
                        .fields
                        .iter()
                        .find(|f| f.name == *field_name)
                        .map(|f| f.ty.clone())
                        .ok_or_else(|| {
                            CompileError::new(
                                "SEM_017",
                                format!("struct '{}' has no field '{}'", name, field_name),
                                Some(*span),
                            )
                        })?;
                    self.check_assign_value(&field_ty, value, *span)?;
                }
                if inits.len() != def.fields.len() {
                    return Err(CompileError::new(
                        "SEM_017",
                        format!(
                            "struct '{}' literal needs {} fields, found {}",
                            name,
                            def.fields.len(),
                            inits.len()
                        ),
                        Some(*span),
                    ));
                }
                Ok(Type::Struct(name.clone()))
            }
            Expr::ArrayLit(elems, span) => {
                let (first, rest) = elems.split_first().ok_or_else(|| {
                    CompileError::new(
                        "SEM_018",
                        "empty array literal cannot infer an element type",
                        Some(*span),
                    )
                })?;
                let elem_ty = self.type_of_expr(first)?;
                for elem in rest {
                    let other = self.type_of_expr(elem)?;
                    if !is_assignable(&elem_ty, &other) {
                        return Err(CompileError::new(
                            "SEM_018",
                            format!(
                                "array literal elements must share one type: {} and {}",
                                elem_ty.name(),
                                other.name()
                            ),
                            Some(*span),
                        ));
                    }
                }
                Ok(Type::Array(Box::new(elem_ty), elems.len()))
            }
            Expr::Binary(op, left, right, span) => {
                let lty = self.type_of_expr(left)?;
                let rty = self.type_of_expr(right)?;

                match op {
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        Ok(Type::Bool)
                    }
                    BinaryOp::And | BinaryOp::Or => Ok(Type::Bool),
                    _ => {
                        if !is_assignable(&lty, &rty) {
                            return Err(CompileError::new(
                                "SEM_003",
                                format!(
                                    "type mismatch in binary operation: {} and {}",
                                    lty.name(),
                                    rty.name()
                                ),
                                Some(*span),
                            ));
                        }
                        Ok(lty)
                    }
                }
            }
            Expr::Unary(op, inner, span) => {
                match op {
                    UnaryOp::AddrOf => {
                        match &**inner {
                            Expr::Ident(name, id_span) => {
                                if self.symbols.lookup_var(name).is_none() {
                                    return Err(CompileError::new(
                                        "SEM_001",
                                        format!("cannot find value '{}' in this scope", name),
                                        Some(*id_span),
                                    ));
                                }
                            }
                            Expr::Unary(UnaryOp::Deref, _, _) => {}
                            Expr::Field(..) | Expr::Index(..) => {}
                            _ => {
                                return Err(CompileError::new(
                                    "SEM_013",
                                    "cannot take the address of a non-variable expression",
                                    Some(*span),
                                ));
                            }
                        }
                        let ty = self.type_of_expr(inner)?;
                        Ok(Type::Pointer(Box::new(ty)))
                    }
                    _ => {
                        let ty = self.type_of_expr(inner)?;
                        match op {
                            UnaryOp::Neg | UnaryOp::BitNot => Ok(ty),
                            UnaryOp::Not => Ok(Type::Bool),
                            UnaryOp::AddrOf => unreachable!(),
                            UnaryOp::Deref => match ty {
                                Type::Pointer(inner_ty) => Ok(*inner_ty),
                                _ => Err(CompileError::new(
                                    "SEM_010",
                                    format!("cannot dereference a non-pointer value of type {}", ty.name()),
                                    Some(*span),
                                )),
                            },
                        }
                    }
                }
            }
            Expr::Call { callee, args, span } => {
                let fn_name = match &**callee {
                    Expr::Ident(name, _) => name.clone(),
                    Expr::Path(parts, _) => parts.join("::"),
                    _ => {
                        return Err(CompileError::new(
                            "SEM_011",
                            "function call target must be an identifier or a path",
                            Some(*span),
                        ))
                    }
                };

                if let Some(fn_sym) = self.symbols.lookup_fn(&fn_name) {
                    if fn_sym.param_types.len() != args.len() {
                        return Err(CompileError::new(
                            "SEM_006",
                            format!(
                                "function '{}' takes {} arguments ({} provided)",
                                fn_name,
                                fn_sym.param_types.len(),
                                args.len()
                            ),
                            Some(*span),
                        ));
                    }
                    for (arg, param_ty) in args.iter().zip(&fn_sym.param_types) {
                        let arg_ty = self.type_of_expr(arg)?;
                        if !is_assignable(param_ty, &arg_ty) {
                            return Err(CompileError::new(
                                "SEM_007",
                                format!(
                                    "argument type mismatch in call to '{}': expected {}, got {}",
                                    fn_name,
                                    param_ty.name(),
                                    arg_ty.name()
                                ),
                                Some(*span),
                            ));
                        }
                    }
                    Ok(fn_sym.ret_type.clone())
                } else {
                    Err(CompileError::new(
                        "SEM_002",
                        format!("cannot find function '{}' in this scope", fn_name),
                        Some(*span),
                    ))
                }
            }
            Expr::Syscall { number, args, .. } => {
                self.type_of_expr(number)?;
                for arg in args {
                    self.type_of_expr(arg)?;
                }
                Ok(Type::I32)
            }
            Expr::Assign { target, value, span } => {
                self.check_mut_target(target)?;
                let target_ty = self.type_of_expr(target)?;
                self.check_assign_value(&target_ty, value, *span)?;
                Ok(target_ty)
            }
        }
    }
}
