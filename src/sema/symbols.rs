use std::collections::HashMap;
use crate::ast::{StructDef, Type};

pub type StructTable = HashMap<String, StructDef>;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub is_mut: bool,
    pub offset: i32, // Stack frame offset relative to EBP (negative for locals, positive for args)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone)]
pub struct ConstSymbol {
    pub name: String,
    pub ty: Type,
    pub value: ConstValue,
}

#[derive(Debug, Clone)]
pub struct FnSymbol {
    pub name: String,
    pub param_types: Vec<Type>,
    pub ret_type: Type,
    pub is_extern: bool,
}

pub struct Scope {
    symbols: HashMap<String, Symbol>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn insert(&mut self, symbol: Symbol) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}

pub struct SymbolTable {
    scopes: Vec<Scope>,
    functions: HashMap<String, FnSymbol>,
    fn_locals: HashMap<String, HashMap<String, Symbol>>,
    constants: HashMap<String, ConstSymbol>,
    pub structs: StructTable,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::new()],
            functions: HashMap::new(),
            fn_locals: HashMap::new(),
            constants: HashMap::new(),
            structs: StructTable::new(),
        }
    }

    pub fn register_struct(&mut self, def: StructDef) {
        self.structs.insert(def.name.clone(), def);
    }

    pub fn fn_vars(&self, fn_name: &str) -> Vec<(&str, i32)> {
        self.fn_locals
            .get(fn_name)
            .map(|m| m.iter().map(|(k, v)| (k.as_str(), v.offset)).collect())
            .unwrap_or_default()
    }

    pub fn lookup_struct(&self, name: &str) -> Option<&StructDef> {
        self.structs.get(name)
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    pub fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn insert_var(&mut self, symbol: Symbol) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(symbol);
        }
    }

    pub fn record_fn_var(&mut self, fn_name: &str, symbol: Symbol) {
        self.insert_var(symbol.clone());
        self.fn_locals
            .entry(fn_name.to_string())
            .or_default()
            .insert(symbol.name.clone(), symbol);
    }

    pub fn lookup_var(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym);
            }
        }
        None
    }

    pub fn lookup_fn_var(&self, fn_name: &str, name: &str) -> Option<&Symbol> {
        self.fn_locals.get(fn_name).and_then(|map| map.get(name))
    }

    pub fn register_fn(&mut self, fn_sym: FnSymbol) {
        self.functions.insert(fn_sym.name.clone(), fn_sym);
    }

    pub fn lookup_fn(&self, name: &str) -> Option<&FnSymbol> {
        self.functions.get(name)
    }

    pub fn register_const(&mut self, const_sym: ConstSymbol) {
        self.constants.insert(const_sym.name.clone(), const_sym);
    }

    pub fn lookup_const(&self, name: &str) -> Option<&ConstSymbol> {
        self.constants.get(name)
    }
}
