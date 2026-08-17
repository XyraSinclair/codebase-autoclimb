use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_detectors::phase::DetectorPhase;
use autoclimb_lang_generic::builtin::all_builtin_configs;
use autoclimb_lang_generic::plugin::{detect_project, GenericLangConfig};
use autoclimb_lang_python::plugin::{detect_python_project, PythonPlugin};
use autoclimb_types::finding::Finding;

pub struct ScanOutput {
    pub lang: String,
    pub files: Vec<String>,
    #[allow(dead_code)]
    pub production_files: Vec<String>,
    pub findings: Vec<Finding>,
    pub potentials: BTreeMap<String, u64>,
}

pub struct ScanConfig<'a> {
    pub exclude: &'a [String],
    pub skip_slow: bool,
    progress: Option<&'a dyn Fn(ScanProgress)>,
}

impl<'a> ScanConfig<'a> {
    pub fn new(exclude: &'a [String], skip_slow: bool) -> Self {
        Self {
            exclude,
            skip_slow,
            progress: None,
        }
    }

    pub(crate) fn with_progress(mut self, progress: &'a dyn Fn(ScanProgress)) -> Self {
        self.progress = Some(progress);
        self
    }

    fn report(&self, progress: ScanProgress) {
        if let Some(report) = self.progress {
            report(progress);
        }
    }
}

pub(crate) enum ScanProgress {
    Started { lang: String },
    FilesDiscovered(usize),
    NoFiles,
    FilesClassified { production: usize, other: usize },
    DependencyGraph(usize),
    PhaseSkipped { label: String },
    PhaseStarted { label: String },
    PhaseFinished { findings: usize },
    FindingsCollected(usize),
}

pub fn collect_scan(
    path: &Path,
    lang_override: Option<&str>,
    config: &ScanConfig<'_>,
) -> Result<ScanOutput, Box<dyn std::error::Error>> {
    let lang = match lang_override {
        Some(lang) => lang.to_owned(),
        None => detect_language(path).ok_or("could not auto-detect language — use --lang")?,
    };
    let runner = resolve_runner(&lang)?;

    config.report(ScanProgress::Started {
        lang: runner.name().to_owned(),
    });

    let files = runner.discover_files(path, config.exclude);
    config.report(ScanProgress::FilesDiscovered(files.len()));

    if files.is_empty() {
        config.report(ScanProgress::NoFiles);
        return Ok(ScanOutput {
            lang,
            files,
            production_files: Vec::new(),
            findings: Vec::new(),
            potentials: BTreeMap::new(),
        });
    }

    let context = runner.build_context(path, files.clone(), config.exclude.to_vec());
    let production_files = context
        .production_files()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    config.report(ScanProgress::FilesClassified {
        production: production_files.len(),
        other: context.file_count() - production_files.len(),
    });

    if let Some(graph) = &context.dep_graph {
        config.report(ScanProgress::DependencyGraph(graph.len()));
    }

    let phases = runner.phases();
    let mut findings = Vec::new();
    let mut potentials = BTreeMap::new();

    for phase in &phases {
        if config.skip_slow && phase.is_slow() {
            config.report(ScanProgress::PhaseSkipped {
                label: phase.label().to_owned(),
            });
            continue;
        }

        config.report(ScanProgress::PhaseStarted {
            label: phase.label().to_owned(),
        });
        let output = phase.run(path, &context)?;
        config.report(ScanProgress::PhaseFinished {
            findings: output.findings.len(),
        });
        findings.extend(output.findings);
        for (key, value) in output.potentials {
            *potentials.entry(key).or_insert(0) += value;
        }
    }

    config.report(ScanProgress::FindingsCollected(findings.len()));

    Ok(ScanOutput {
        lang,
        files,
        production_files,
        findings,
        potentials,
    })
}

/// Auto-detect the project language by checking marker files.
fn detect_language(root: &Path) -> Option<String> {
    if detect_python_project(root) {
        return Some("python".into());
    }

    for config in all_builtin_configs() {
        if detect_project(root, &config) {
            return Some(config.name);
        }
    }

    None
}

enum LangRunner {
    Python(PythonPlugin),
    Generic(Box<GenericLangConfig>),
}

impl LangRunner {
    fn name(&self) -> &str {
        match self {
            LangRunner::Python(_) => "python",
            LangRunner::Generic(config) => config.name(),
        }
    }

    fn discover_files(&self, root: &Path, exclude: &[String]) -> Vec<String> {
        match self {
            LangRunner::Python(plugin) => plugin.discover_files(root, exclude),
            LangRunner::Generic(config) => config.discover_files(root, exclude),
        }
    }

    fn build_context(
        &self,
        root: &Path,
        files: Vec<String>,
        exclusions: Vec<String>,
    ) -> autoclimb_detectors::context::ScanContext {
        match self {
            LangRunner::Python(plugin) => plugin.build_context(root, files, exclusions),
            LangRunner::Generic(config) => config.build_context(root, files, exclusions),
        }
    }

    fn phases(&self) -> Vec<Box<dyn DetectorPhase>> {
        match self {
            LangRunner::Python(plugin) => plugin.phases(),
            LangRunner::Generic(config) => config.phases(),
        }
    }
}

fn resolve_runner(lang: &str) -> Result<LangRunner, Box<dyn std::error::Error>> {
    if lang == "python" {
        return Ok(LangRunner::Python(PythonPlugin));
    }

    for config in all_builtin_configs() {
        if config.name == lang {
            return Ok(LangRunner::Generic(Box::new(config)));
        }
    }

    Err(format!("unsupported language: {lang}").into())
}
