//! Fixer registry — lookup fixers by name or detector.

use crate::python::PythonDebugLogsFixer;
use crate::python::PythonUnusedImportsFixer;
use crate::typescript::TypeScriptLogsFixer;
use crate::typescript::TypeScriptUnusedImportsFixer;
use crate::Fixer;

/// Registry of available fixers.
pub struct FixerRegistry {
    fixers: Vec<Box<dyn Fixer>>,
}

impl FixerRegistry {
    /// Create a registry with all built-in fixers.
    pub fn new() -> Self {
        Self {
            fixers: vec![
                Box::new(PythonUnusedImportsFixer),
                Box::new(PythonDebugLogsFixer),
                Box::new(TypeScriptLogsFixer),
                Box::new(TypeScriptUnusedImportsFixer),
            ],
        }
    }

    /// Find a fixer by name.
    pub fn get(&self, name: &str) -> Option<&dyn Fixer> {
        self.fixers
            .iter()
            .find(|f| f.name() == name)
            .map(|f| f.as_ref())
    }

    /// Find all fixers that can address findings from a given detector.
    pub fn for_detector(&self, detector: &str) -> Vec<&dyn Fixer> {
        self.fixers
            .iter()
            .filter(|f| f.detector() == detector)
            .map(|f| f.as_ref())
            .collect()
    }

    /// List all registered fixers.
    pub fn all(&self) -> Vec<&dyn Fixer> {
        self.fixers.iter().map(|f| f.as_ref()).collect()
    }

    /// Number of registered fixers.
    pub fn len(&self) -> usize {
        self.fixers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fixers.is_empty()
    }
}

impl Default for FixerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_fixers() {
        let reg = FixerRegistry::new();
        assert!(reg.len() >= 4);
    }

    #[test]
    fn lookup_by_name() {
        let reg = FixerRegistry::new();
        assert!(reg.get("python-unused-imports").is_some());
        assert!(reg.get("ts-logs").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn lookup_by_detector() {
        let reg = FixerRegistry::new();
        let fixers = reg.for_detector("unused");
        assert!(!fixers.is_empty());
    }
}
