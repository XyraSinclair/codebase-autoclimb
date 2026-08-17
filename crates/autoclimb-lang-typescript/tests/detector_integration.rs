//! Integration tests for TypeScript detectors.

use std::collections::BTreeSet;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::DetectorPhase;
use autoclimb_discovery::zones::ZoneMap;
use autoclimb_lang_typescript::logs::TypeScriptLogsDetector;
use autoclimb_lang_typescript::react::ReactPatternDetector;
use autoclimb_lang_typescript::security::TypeScriptSecurityDetector;
use autoclimb_lang_typescript::smells::TypeScriptSmellsDetector;
use autoclimb_lang_typescript::unused::TypeScriptUnusedDetector;

fn make_ctx(files: Vec<String>) -> ScanContext {
    let zone_map = ZoneMap::new(&files, &[]);
    ScanContext {
        lang_name: "typescript".into(),
        files,
        dep_graph: None,
        zone_map,
        exclusions: vec![],
        entry_patterns: vec!["index".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

#[test]
fn smells_detector_finds_empty_catch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("app.ts"),
        "try {\n  doSomething();\n} catch (e) {}\n",
    )
    .unwrap();

    let ctx = make_ctx(vec!["app.ts".into()]);
    let detector = TypeScriptSmellsDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.summary.to_lowercase().contains("empty catch")),
        "should detect empty catch block"
    );
}

#[test]
fn security_detector_finds_eval() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("app.ts"), "const result = eval(userInput);\n").unwrap();

    let ctx = make_ctx(vec!["app.ts".into()]);
    let detector = TypeScriptSecurityDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        output.findings.iter().any(|f| f.summary.contains("eval")),
        "should detect eval usage"
    );
}

#[test]
fn logs_detector_finds_console_statements() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("app.ts"),
        "const x = 1;\nconsole.log('debug');\nconsole.warn('warn');\nconsole.error('error');\n",
    )
    .unwrap();

    let ctx = make_ctx(vec!["app.ts".into()]);
    let detector = TypeScriptLogsDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        !output.findings.is_empty(),
        "should detect console statements"
    );
}

#[test]
fn react_detector_finds_missing_key() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("List.tsx"),
        "function List({ items }: { items: string[] }) {\n  return <ul>{items.map(i => <li>{i}</li>)}</ul>;\n}\n",
    )
    .unwrap();

    let ctx = make_ctx(vec!["List.tsx".into()]);
    let detector = ReactPatternDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        output.findings.iter().any(|f| f.summary.contains("key")),
        "should detect missing key prop in .map()"
    );
}

#[test]
fn unused_detector_finds_unused_import() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("app.ts"),
        "import { useState } from 'react';\nimport { useEffect } from 'react';\n\nconst x = useState(0);\n",
    )
    .unwrap();

    let ctx = make_ctx(vec!["app.ts".into()]);
    let detector = TypeScriptUnusedDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.summary.contains("useEffect")),
        "should detect unused useEffect import"
    );
}

#[test]
fn all_detectors_handle_empty_files_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let ctx = make_ctx(vec![]);

    let detectors: Vec<Box<dyn DetectorPhase>> = vec![
        Box::new(TypeScriptSmellsDetector),
        Box::new(TypeScriptSecurityDetector),
        Box::new(TypeScriptLogsDetector),
        Box::new(ReactPatternDetector),
        Box::new(TypeScriptUnusedDetector),
    ];

    for detector in &detectors {
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }
}

#[test]
fn smells_detector_finds_any_type() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("app.ts"),
        "function process(data: any): any {\n  return data;\n}\n",
    )
    .unwrap();

    let ctx = make_ctx(vec!["app.ts".into()]);
    let detector = TypeScriptSmellsDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        output.findings.iter().any(|f| f.summary.contains("any")),
        "should detect 'any' type usage"
    );
}

#[test]
fn security_detector_finds_dangerously_set_inner_html() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("app.tsx"),
        "<div dangerouslySetInnerHTML={{ __html: userInput }} />\n",
    )
    .unwrap();

    let ctx = make_ctx(vec!["app.tsx".into()]);
    let detector = TypeScriptSecurityDetector;
    let output = detector.run(root, &ctx).unwrap();
    assert!(
        output
            .findings
            .iter()
            .any(|f| f.summary.contains("dangerouslySetInnerHTML")),
        "should detect dangerouslySetInnerHTML"
    );
}
