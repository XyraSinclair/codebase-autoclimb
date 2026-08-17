use std::collections::BTreeMap;

/// Information about a function extracted from source code.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub params: Vec<String>,
    pub return_annotation: Option<String>,
}

/// Information about a class extracted from source code.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub loc: u32,
    pub methods: Vec<String>,
    pub metrics: BTreeMap<String, f64>,
}
