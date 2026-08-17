//! ASCII tree rendering for project structure with findings overlay.
//!
//! Renders a file tree showing finding counts per directory/file.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use autoclimb_types::finding::Finding;

/// Render a project tree with finding counts overlaid.
pub fn render_tree(files: &[String], findings: &BTreeMap<String, Finding>) -> String {
    // Count findings per file
    let mut file_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for finding in findings.values() {
        if finding.status == autoclimb_types::enums::Status::Open {
            *file_counts.entry(&finding.file).or_default() += 1;
        }
    }

    // Build tree structure
    let mut tree = TreeNode::new(".");
    for file in files {
        tree.insert(file, *file_counts.get(file.as_str()).unwrap_or(&0));
    }

    let mut output = String::new();
    tree.render(&mut output, "", true);
    output
}

struct TreeNode {
    name: String,
    count: usize,
    children: BTreeMap<String, TreeNode>,
}

impl TreeNode {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            count: 0,
            children: BTreeMap::new(),
        }
    }

    fn insert(&mut self, path: &str, count: usize) {
        let components: Vec<&str> = Path::new(path)
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();

        if components.is_empty() {
            return;
        }

        self.insert_parts(&components, count);
    }

    fn insert_parts(&mut self, parts: &[&str], count: usize) {
        if parts.is_empty() {
            return;
        }

        let key = parts[0].to_string();
        let child = self
            .children
            .entry(key.clone())
            .or_insert_with(|| TreeNode::new(&key));

        if parts.len() == 1 {
            child.count = count;
        } else {
            child.insert_parts(&parts[1..], count);
        }
    }

    fn total_count(&self) -> usize {
        self.count
            + self
                .children
                .values()
                .map(|c| c.total_count())
                .sum::<usize>()
    }

    fn render(&self, out: &mut String, prefix: &str, is_root: bool) {
        if is_root {
            let total = self.total_count();
            out.push_str(&self.name);
            if total > 0 {
                out.push_str(&format!("  ({total} findings)"));
            }
            out.push('\n');
        }

        let entries: Vec<_> = self.children.iter().collect();
        for (i, (name, child)) in entries.iter().enumerate() {
            let is_last = i == entries.len() - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let total = child.total_count();
            let count_str = if child.children.is_empty() {
                // Leaf file
                if child.count > 0 {
                    format!("  [{} findings]", child.count)
                } else {
                    String::new()
                }
            } else {
                // Directory
                if total > 0 {
                    format!("  ({total})")
                } else {
                    String::new()
                }
            };

            out.push_str(&format!("{prefix}{connector}{name}{count_str}\n"));

            let new_prefix = format!("{prefix}{child_prefix}");
            child.render_children(out, &new_prefix);
        }
    }

    fn render_children(&self, out: &mut String, prefix: &str) {
        let entries: Vec<_> = self.children.iter().collect();
        for (i, (name, child)) in entries.iter().enumerate() {
            let is_last = i == entries.len() - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let total = child.total_count();
            let count_str = if child.children.is_empty() {
                if child.count > 0 {
                    format!("  [{} findings]", child.count)
                } else {
                    String::new()
                }
            } else if total > 0 {
                format!("  ({total})")
            } else {
                String::new()
            };

            out.push_str(&format!("{prefix}{connector}{name}{count_str}\n"));

            let new_prefix = format!("{prefix}{child_prefix}");
            child.render_children(out, &new_prefix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(file: &str) -> Finding {
        Finding {
            id: format!("test::{file}::issue"),
            detector: "test".into(),
            file: file.into(),
            tier: Tier::QuickFix,
            confidence: Confidence::High,
            summary: "test finding".into(),
            detail: serde_json::json!({}),
            status: Status::Open,
            note: None,
            first_seen: String::new(),
            last_seen: String::new(),
            resolved_at: None,
            reopen_count: 0,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn renders_basic_tree() {
        let files = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "tests/test_main.rs".to_string(),
        ];
        let mut findings = BTreeMap::new();
        let f = make_finding("src/main.rs");
        findings.insert(f.id.clone(), f);

        let tree = render_tree(&files, &findings);
        assert!(tree.contains("src"));
        assert!(tree.contains("main.rs"));
        assert!(tree.contains("[1 findings]"));
    }

    #[test]
    fn empty_tree() {
        let tree = render_tree(&[], &BTreeMap::new());
        assert!(tree.contains("."));
    }

    #[test]
    fn nested_dirs() {
        let files = vec![
            "a/b/c.rs".to_string(),
            "a/b/d.rs".to_string(),
            "a/e.rs".to_string(),
        ];
        let tree = render_tree(&files, &BTreeMap::new());
        assert!(tree.contains("a"));
        assert!(tree.contains("b"));
        assert!(tree.contains("c.rs"));
    }
}
