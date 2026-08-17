use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, Subcommand};

use autoclimb_config::{load_or_default, save_config};
use autoclimb_detectors::phase::DetectorPhase;
use autoclimb_lang_generic::builtin::all_builtin_configs;
use autoclimb_lang_generic::plugin::detect_project;
use autoclimb_lang_python::plugin::{detect_python_project, PythonPlugin};
use autoclimb_output::{
    cli_command, colorize, format_analysis_summary, format_assessment_status, format_diff,
    format_dimension_deltas, format_dimension_table_with_strict, format_finding,
    format_score_quartet, format_strict_target, format_tier_summary,
};
use autoclimb_plan::plan_model::PlanModel;
use autoclimb_plan::ranking::{build_queue, QueueBuildOptions};
use autoclimb_state::filtering::apply_noise_budget;
use autoclimb_state::merge::{merge_scan, MergeOpts};
use autoclimb_state::persist::{load_or_create, save_state};
use autoclimb_types::enums::Status;
use autoclimb_types::finding::Finding;
use autoclimb_types::registry::{detector_by_name, DETECTORS};

mod explore;
mod scan_engine;

use explore::ExploreArgs;
use scan_engine::{collect_scan, ScanConfig, ScanProgress};

#[derive(Parser)]
#[command(
    name = "autoclimb",
    version,
    about = "Autonomous, proof-carrying hill-climb of a codebase"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a codebase for code health issues
    Scan(ScanArgs),
    /// Record a snapshot-bound exploration
    Explore(ExploreArgs),
    /// Display findings with scoping and filtering
    Show(ShowArgs),
    /// Score dashboard and project overview
    Status(StatusArgs),
    /// Prioritized action suggestions
    Next(NextArgs),
    /// Mark findings as fixed/wontfix/false_positive
    Resolve(ResolveArgs),
    /// Manage project configuration
    Config(ConfigArgs),
    /// List supported languages
    Langs,
    /// Run or manage LLM code review
    Review(ReviewArgs),
    /// Apply automated fixes for findings
    Fix(FixArgs),
    /// Move/reorganize files with import updates
    Move(MoveArgs),
    /// Show prioritized work queue
    Queue(QueueArgs),
    /// Show or manage the living plan
    Plan(PlanArgs),
    /// Show project file tree with findings overlay
    Tree(TreeArgs),
    /// Generate HTML visualization report
    Viz(VizArgs),
    /// Run a single detector by name
    Detect(DetectArgs),
    /// Manage file exclusion patterns
    Exclude(ExcludeArgs),
}

// ── Scan ────────────────────────────────────────────────

#[derive(clap::Args)]
struct ScanArgs {
    /// Path to scan (default: current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Language to scan (default: auto-detect)
    #[arg(short, long)]
    lang: Option<String>,

    /// Skip slow detector phases
    #[arg(long)]
    skip_slow: bool,

    /// Force-resolve all disappeared findings
    #[arg(long)]
    force_resolve: bool,

    /// Exclusion patterns
    #[arg(long)]
    exclude: Vec<String>,

    /// State file path (default: .autoclimb/state.json)
    #[arg(long)]
    state: Option<PathBuf>,
}

// ── Show ────────────────────────────────────────────────

#[derive(clap::Args)]
struct ShowArgs {
    /// Pattern to filter findings (substring match on summary or file)
    pattern: Option<String>,

    /// Filter by detector name
    #[arg(long)]
    detector: Option<String>,

    /// Filter by status (open/fixed/wontfix/false_positive)
    #[arg(long)]
    status: Option<String>,

    /// Filter by file path prefix
    #[arg(long)]
    file: Option<String>,

    /// Filter by tier (1-4)
    #[arg(long)]
    tier: Option<u8>,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Disable noise budget
    #[arg(long)]
    no_noise_budget: bool,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Status ──────────────────────────────────────────────

#[derive(clap::Args)]
struct StatusArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Next ────────────────────────────────────────────────

#[derive(clap::Args)]
struct NextArgs {
    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Resolve ─────────────────────────────────────────────

#[derive(clap::Args)]
struct ResolveArgs {
    /// Finding ID to resolve
    finding_id: String,

    /// New status: fixed, wontfix, or false_positive
    #[arg(long)]
    status: String,

    /// Optional note explaining the resolution
    #[arg(long)]
    note: Option<String>,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Config ──────────────────────────────────────────────

#[derive(clap::Args)]
struct ConfigArgs {
    #[command(subcommand)]
    action: ConfigAction,

    /// Path to project root
    #[arg(short, long, global = true)]
    path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Get a config value
    Get { key: String },
    /// Set a config value
    Set { key: String, value: String },
    /// Reset a config key to its default
    Unset { key: String },
    /// List all config values
    List,
}

// ── Review ─────────────────────────────────────────────

#[derive(clap::Args)]
struct ReviewArgs {
    /// Prepare review batches
    #[arg(long)]
    prepare: bool,

    /// Run review batches
    #[arg(long)]
    run_batches: bool,

    /// Start external review session
    #[arg(long)]
    external_start: bool,

    /// Submit external review results
    #[arg(long)]
    external_submit: Option<String>,

    /// Import review findings from JSON
    #[arg(long)]
    import: Option<PathBuf>,

    /// Import mode: trusted, attested, findings_only
    #[arg(long, default_value = "findings_only")]
    mode: String,

    /// Bypass rerun safety gates when review context is intentionally stale
    #[arg(long)]
    force_review_rerun: bool,

    /// Backend runner name
    #[arg(long, default_value = "codex")]
    backend: String,

    /// Runner name for external sessions
    #[arg(long, default_value = "claude")]
    runner: String,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Fix ────────────────────────────────────────────────

#[derive(clap::Args)]
struct FixArgs {
    /// Pattern to filter findings
    pattern: Option<String>,

    /// Filter by detector name
    #[arg(long)]
    detector: Option<String>,

    /// Dry run — show what would be fixed
    #[arg(long)]
    dry_run: bool,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Move ────────────────────────────────────────────────

#[derive(clap::Args)]
struct MoveArgs {
    /// Source file or directory path
    source: String,

    /// Destination file or directory path
    dest: String,

    /// Dry run — show what would change
    #[arg(long)]
    dry_run: bool,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Queue ───────────────────────────────────────────────

#[derive(clap::Args)]
struct QueueArgs {
    /// Maximum number of items to show
    #[arg(long, default_value = "20")]
    count: usize,

    /// Filter by tier (1-4)
    #[arg(long)]
    tier: Option<u8>,

    /// Filter by scope (file:<path> or detector:<name>)
    #[arg(long)]
    scope: Option<String>,

    /// Show only chronic reopeners (2+ reopens)
    #[arg(long)]
    chronic: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Plan ────────────────────────────────────────────────

#[derive(clap::Args)]
struct PlanArgs {
    #[command(subcommand)]
    action: PlanAction,

    /// Path to project root
    #[arg(short, long, global = true)]
    path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum PlanAction {
    /// Show the current plan summary
    Show,
    /// Skip finding(s) from the work queue
    Skip(PlanSkipArgs),
    /// Unskip finding(s), returning them to the queue
    Unskip(PlanUnskipArgs),
    /// Move finding(s) to a specific position in the queue
    Move(PlanMoveArgs),
    /// Reset the plan to empty
    Reset,
}

#[derive(clap::Args)]
struct PlanSkipArgs {
    /// Finding IDs to skip
    finding_ids: Vec<String>,
    /// Reason for skipping
    #[arg(long)]
    reason: Option<String>,
    /// Skip permanently (default: temporary)
    #[arg(long)]
    permanent: bool,
}

#[derive(clap::Args)]
struct PlanUnskipArgs {
    /// Finding IDs to unskip
    finding_ids: Vec<String>,
}

#[derive(clap::Args)]
struct PlanMoveArgs {
    /// Finding IDs to move (placed at start of queue in given order)
    finding_ids: Vec<String>,
}

// ── Tree ───────────────────────────────────────────────

#[derive(clap::Args)]
struct TreeArgs {
    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Viz ────────────────────────────────────────────────

#[derive(clap::Args)]
struct VizArgs {
    /// Output HTML file path
    #[arg(short, long, default_value = "autoclimb-report.html")]
    output: PathBuf,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,
}

// ── Detect ─────────────────────────────────────────────

#[derive(clap::Args)]
struct DetectArgs {
    /// Detector name to run
    name: String,

    /// Path to project root
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Language override
    #[arg(short, long)]
    lang: Option<String>,

    /// Output as JSON
    #[arg(long)]
    json: bool,
}

// ── Exclude ────────────────────────────────────────────

#[derive(clap::Args)]
struct ExcludeArgs {
    #[command(subcommand)]
    action: ExcludeAction,

    /// Path to project root
    #[arg(short, long, global = true)]
    path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum ExcludeAction {
    /// List current exclusion patterns
    List,
    /// Add an exclusion pattern
    Add { pattern: String },
    /// Remove an exclusion pattern
    Remove { pattern: String },
}

// ── Path helpers ────────────────────────────────────────

fn resolve_root(path: Option<PathBuf>) -> PathBuf {
    let root = path.unwrap_or_else(|| std::env::current_dir().unwrap());
    root.canonicalize().unwrap_or(root)
}

fn state_path(root: &std::path::Path) -> PathBuf {
    root.join(".autoclimb").join("state.json")
}

fn config_path(root: &std::path::Path) -> PathBuf {
    root.join(".autoclimb").join("config.json")
}

fn initialize_invocation_command() {
    if std::env::var_os("AUTOCLIMB_CMD").is_some() {
        return;
    }

    if let Some(argv0) = std::env::args().next() {
        std::env::set_var("AUTOCLIMB_CMD", argv0);
    }
}

fn render_guidance(text: &str) -> String {
    let base = cli_command("");
    text.replace("autoclimb ", &format!("{base} "))
}

fn merge_excludes(config_excludes: &[String], cli_excludes: &[String]) -> Vec<String> {
    let mut merged = config_excludes.to_vec();
    for pattern in cli_excludes {
        if !merged.contains(pattern) {
            merged.push(pattern.clone());
        }
    }
    merged
}

fn require_completed_scan(
    state: &autoclimb_types::state::StateModel,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if state.scan_count > 0 {
        return Ok(());
    }

    Err(format!(
        "{command} requires a completed scan — run `{}` first",
        cli_command("scan")
    )
    .into())
}

fn review_target_files(state: &autoclimb_types::state::StateModel) -> Vec<String> {
    state
        .findings
        .values()
        .filter(|f| f.status == Status::Open && !f.suppressed)
        .map(|f| f.file.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn run_review_preflight(
    state: &autoclimb_types::state::StateModel,
    force_review_rerun: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = autoclimb_review::preflight::preflight_check(state, force_review_rerun);

    for message in &result.messages {
        let style = if message.starts_with("Note:") {
            "yellow"
        } else {
            "red"
        };
        println!("  {}", colorize(message, style));
    }

    if result.ok {
        Ok(())
    } else {
        Err("review preflight failed".into())
    }
}

fn run_codex_review_batches(
    prompts: &[autoclimb_review::types::BatchPrompt],
    root: &std::path::Path,
) -> Result<Vec<autoclimb_review::types::BatchResult>, Box<dyn std::error::Error>> {
    let runner = autoclimb_review::runner_codex::CodexRunner::default();
    let opts = autoclimb_review::runner::RunnerOpts {
        cwd: Some(root.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let prompt_pairs: Vec<(usize, String)> = prompts
        .iter()
        .map(|prompt| (prompt.index, prompt.prompt.clone()))
        .collect();

    let rt = tokio::runtime::Runtime::new()?;
    let mut results = rt.block_on(autoclimb_review::runner::execute_batches(
        &runner,
        &prompt_pairs,
        &opts,
        1,
    ));

    let allowed_dimensions: Vec<String> = autoclimb_review::dimensions::DimensionRegistry::new()
        .all_keys()
        .into_iter()
        .map(str::to_string)
        .collect();

    for (result, prompt) in results.iter_mut().zip(prompts.iter()) {
        if result.status != autoclimb_review::types::BatchStatus::Success
            || result.payload.is_some()
        {
            continue;
        }

        if let Err(err) = autoclimb_review::result_parser::attach_parsed_payload(
            result,
            prompt,
            &allowed_dimensions,
            "codex",
        ) {
            result.status = autoclimb_review::types::BatchStatus::ParseError;
            result.raw_output = format!("parse error: {err}\n\n{}", result.raw_output);
        }
    }

    Ok(results)
}

// ── Main ────────────────────────────────────────────────

fn main() {
    initialize_invocation_command();
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Scan(args) => run_scan(args),
        Commands::Explore(args) => explore::run(args),
        Commands::Show(args) => run_show(args),
        Commands::Status(args) => run_status(args),
        Commands::Next(args) => run_next(args),
        Commands::Resolve(args) => run_resolve(args),
        Commands::Config(args) => run_config(args),
        Commands::Langs => run_langs(),
        Commands::Review(args) => run_review(args),
        Commands::Fix(args) => run_fix(args),
        Commands::Move(args) => run_move(args),
        Commands::Queue(args) => run_queue(args),
        Commands::Plan(args) => run_plan(args),
        Commands::Tree(args) => run_tree(args),
        Commands::Viz(args) => run_viz(args),
        Commands::Detect(args) => run_detect(args),
        Commands::Exclude(args) => run_exclude(args),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ── Scan command ────────────────────────────────────────

fn run_scan(args: ScanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let root = resolve_root(args.path);

    let sp = args.state.unwrap_or_else(|| state_path(&root));

    let mut state = load_or_create(&sp)?;

    let project_config = load_or_default(&config_path(&root));
    let effective_exclude = merge_excludes(&project_config.exclude, &args.exclude);
    let report_progress = |progress| match progress {
        ScanProgress::Started { lang } => println!("  scanning {} ({lang})", root.display()),
        ScanProgress::FilesDiscovered(count) => println!("  found {count} files"),
        ScanProgress::NoFiles => println!("  no files found — nothing to scan"),
        ScanProgress::FilesClassified { production, other } => {
            println!("  {production} production, {other} non-production");
        }
        ScanProgress::DependencyGraph(nodes) => println!("  dep graph: {nodes} nodes"),
        ScanProgress::PhaseSkipped { label } => println!("  skipping {label} (slow)"),
        ScanProgress::PhaseStarted { label } => print!("  running {label}..."),
        ScanProgress::PhaseFinished { findings } => println!(" {findings} findings"),
        ScanProgress::FindingsCollected(count) => println!("  total raw findings: {count}"),
    };
    let scan_config =
        ScanConfig::new(&effective_exclude, args.skip_slow).with_progress(&report_progress);
    let scan = collect_scan(&root, args.lang.as_deref(), &scan_config)?;

    if scan.files.is_empty() {
        return Ok(());
    }

    let opts = MergeOpts {
        lang: Some(scan.lang),
        scan_path: None,
        force_resolve: args.force_resolve,
        exclude: effective_exclude,
        potentials: Some(scan.potentials),
        merge_potentials: false,
        include_slow: !args.skip_slow,
        ignore: None,
    };

    let diff = merge_scan(&mut state, scan.findings, &opts);
    save_state(&state, &sp)?;
    let elapsed = start.elapsed();

    println!();
    println!("  scan complete ({:.1}s)", elapsed.as_secs_f64());
    println!("  ──────────────────────────────────");
    println!(
        "{}",
        format_score_quartet(
            state.overall_score,
            state.objective_score,
            state.strict_score,
            state.verified_strict_score,
        )
    );
    println!("{}", format_tier_summary(&state.stats));
    println!("{}", format_diff(&diff));

    // Dimension breakdown with deltas
    if let Some(ref dims) = state.dimension_scores {
        type DimMap = BTreeMap<String, autoclimb_types::scoring::DimensionScoreEntry>;
        let prev_dims = state
            .extra
            .get("prev_dimension_scores")
            .and_then(|v| serde_json::from_value::<DimMap>(v.clone()).ok());
        println!();
        println!("{}", format_dimension_deltas(dims, prev_dims.as_ref()));
    }

    // Analysis summary (top issues by detector)
    println!();
    println!("{}", format_analysis_summary(&state.findings));

    // Subjective assessment status
    let assessment_count = state
        .extra
        .get("subjective_assessments")
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .unwrap_or(0);
    let dim_count = state
        .dimension_scores
        .as_ref()
        .map(|d| d.len())
        .unwrap_or(0);
    println!(
        "{}",
        format_assessment_status(assessment_count > 0, assessment_count, dim_count)
    );

    // Strict target
    println!(
        "{}",
        format_strict_target(
            state.strict_score,
            project_config.target_strict_score as f64,
        )
    );
    println!("  state: {}", sp.display());

    Ok(())
}

// ── Show command ────────────────────────────────────────

fn run_show(args: ShowArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let state = load_or_create(&state_path(&root))?;
    let config = load_or_default(&config_path(&root));

    // Collect and filter findings
    let mut filtered: Vec<&Finding> = state
        .findings
        .values()
        .filter(|f| {
            // Status filter
            if let Some(ref s) = args.status {
                if f.status.as_str() != s.as_str() {
                    return false;
                }
            }
            // Detector filter
            if let Some(ref d) = args.detector {
                if f.detector != *d {
                    return false;
                }
            }
            // File filter
            if let Some(ref fp) = args.file {
                if !f.file.starts_with(fp.as_str()) {
                    return false;
                }
            }
            // Tier filter
            if let Some(t) = args.tier {
                if f.tier.as_u8() != t {
                    return false;
                }
            }
            // Pattern filter
            if let Some(ref pat) = args.pattern {
                let pat_lower = pat.to_lowercase();
                if !f.summary.to_lowercase().contains(&pat_lower)
                    && !f.file.to_lowercase().contains(&pat_lower)
                    && !f.id.to_lowercase().contains(&pat_lower)
                {
                    return false;
                }
            }
            // Skip suppressed
            if f.suppressed {
                return false;
            }
            true
        })
        .collect();

    // Apply noise budget unless disabled
    if !args.no_noise_budget {
        filtered = apply_noise_budget(
            &filtered,
            config.finding_noise_budget,
            config.finding_noise_global_budget,
        );
    }

    if args.json {
        let json_findings: Vec<&Finding> = filtered;
        println!("{}", serde_json::to_string_pretty(&json_findings)?);
        return Ok(());
    }

    if filtered.is_empty() {
        println!("  No findings match the given filters.");
        return Ok(());
    }

    // Group by file
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in &filtered {
        by_file.entry(&f.file).or_default().push(f);
    }

    println!(
        "  {} findings{}:",
        filtered.len(),
        if !args.no_noise_budget {
            " (noise-budgeted)"
        } else {
            ""
        }
    );
    println!();

    for (file, findings) in &by_file {
        println!(
            "  {}  {}",
            colorize(file, "cyan"),
            colorize(&format!("({} findings)", findings.len()), "dim"),
        );
        for f in findings {
            println!("{}", format_finding(f, true));
        }
        println!();
    }

    Ok(())
}

// ── Status command ──────────────────────────────────────

fn run_status(args: StatusArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let state = load_or_create(&state_path(&root))?;
    let config = load_or_default(&config_path(&root));

    if args.json {
        let output = serde_json::json!({
            "overall_score": state.overall_score,
            "objective_score": state.objective_score,
            "strict_score": state.strict_score,
            "verified_strict_score": state.verified_strict_score,
            "stats": state.stats,
            "dimension_scores": state.dimension_scores,
            "strict_dimension_scores": state.strict_dimension_scores,
            "target_strict_score": config.target_strict_score,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!();
    println!(
        "{}",
        format_score_quartet(
            state.overall_score,
            state.objective_score,
            state.strict_score,
            state.verified_strict_score,
        )
    );
    println!();

    if let Some(ref dims) = state.dimension_scores {
        println!(
            "{}",
            format_dimension_table_with_strict(dims, state.strict_dimension_scores.as_ref())
        );
        println!();
    }

    println!("{}", format_tier_summary(&state.stats));
    println!();
    println!(
        "{}",
        format_strict_target(state.strict_score, config.target_strict_score as f64)
    );

    if let Some(ref last) = state.last_scan {
        println!("  {}", colorize(&format!("Last scan: {last}"), "dim"));
    }

    Ok(())
}

// ── Next command ────────────────────────────────────────

fn run_next(args: NextArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let state = load_or_create(&state_path(&root))?;
    let config = load_or_default(&config_path(&root));

    if args.json {
        // Collect suggestions as JSON
        let suggestions = build_suggestions(&state, &config);
        println!("{}", serde_json::to_string_pretty(&suggestions)?);
        return Ok(());
    }

    let suggestions = build_suggestions(&state, &config);

    if suggestions.is_empty() {
        println!(
            "  {}",
            colorize(
                &format!("No suggestions — run `{}` first.", cli_command("scan")),
                "dim",
            )
        );
        return Ok(());
    }

    println!();
    println!("  {}", colorize("Next actions:", "bold"));
    println!();

    for (i, s) in suggestions.iter().enumerate() {
        let num = i + 1;
        let action = s["action"].as_str().unwrap_or("");
        let detail = s["detail"].as_str().unwrap_or("");
        let cmd = s["command"].as_str().unwrap_or("");
        println!("  {num}. {action}: {detail}");
        if !cmd.is_empty() {
            println!("     {}", colorize(&format!("`{cmd}`"), "dim"));
        }
    }
    println!();

    Ok(())
}

fn build_suggestions(
    state: &autoclimb_types::state::StateModel,
    config: &autoclimb_config::ProjectConfig,
) -> Vec<serde_json::Value> {
    let mut suggestions = Vec::new();

    // Suggest scan if no scan yet
    if state.last_scan.is_none() {
        suggestions.push(serde_json::json!({
            "action": "scan",
            "detail": "No scan data yet",
            "command": cli_command("scan"),
        }));
        return suggestions;
    }

    // Find lowest-scoring dimensions
    if let Some(ref dims) = state.dimension_scores {
        let mut dim_list: Vec<(&String, &autoclimb_types::scoring::DimensionScoreEntry)> =
            dims.iter().collect();
        dim_list.sort_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap());

        for (name, entry) in dim_list.iter().take(3) {
            if entry.score < config.target_strict_score as f64 {
                // Find the detector with most issues in this dimension
                let dim_detectors: Vec<&str> = DETECTORS
                    .iter()
                    .filter(|d| d.dimension == name.as_str())
                    .map(|d| d.name)
                    .collect();

                let best_detector = dim_detectors.first().copied().unwrap_or("show");
                suggestions.push(serde_json::json!({
                    "action": "fix",
                    "detail": format!("{name} at {:.1}% — below target {}", entry.score, config.target_strict_score),
                    "command": cli_command(&format!("show --detector {best_detector}")),
                }));
            }
        }
    }

    // Find highest-impact detectors by open count
    let mut det_counts: BTreeMap<&str, u64> = BTreeMap::new();
    for f in state.findings.values() {
        if f.status == Status::Open && !f.suppressed {
            *det_counts.entry(&f.detector).or_insert(0) += 1;
        }
    }
    let mut det_list: Vec<(&&str, &u64)> = det_counts.iter().collect();
    det_list.sort_by(|a, b| b.1.cmp(a.1));

    for (det, count) in det_list.iter().take(2) {
        if let Some(meta) = detector_by_name(det) {
            suggestions.push(serde_json::json!({
                "action": meta.action_type.label(),
                "detail": format!("{} open {} findings — {}", count, det, render_guidance(meta.guidance)),
                "command": cli_command(&format!("show --detector {det}")),
            }));
        }
    }

    // Cap at 5 suggestions
    suggestions.truncate(5);
    suggestions
}

// ── Resolve command ─────────────────────────────────────

fn run_resolve(args: ResolveArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let mut state = load_or_create(&sp)?;

    let new_status = match args.status.as_str() {
        "fixed" => Status::Fixed,
        "wontfix" => Status::Wontfix,
        "false_positive" => Status::FalsePositive,
        other => {
            return Err(
                format!("invalid status: {other} (use fixed/wontfix/false_positive)").into(),
            )
        }
    };

    let finding = state
        .findings
        .get_mut(&args.finding_id)
        .ok_or_else(|| format!("finding not found: {}", args.finding_id))?;

    let old_status = finding.status;
    finding.status = new_status;
    finding.resolved_at = Some(autoclimb_types::newtypes::Timestamp::now().0);
    if let Some(note) = args.note {
        finding.note = Some(note);
    }

    // Recompute scores
    let potentials: BTreeMap<String, u64> = state
        .potentials
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
        .collect();
    let bundle = autoclimb_scoring::results::compute_score_bundle(&state.findings, &potentials);

    state.overall_score = bundle.overall_score;
    state.objective_score = bundle.objective_score;
    state.strict_score = bundle.strict_score;
    state.verified_strict_score = bundle.verified_strict_score;
    state.dimension_scores = Some(bundle.dimension_scores);
    state.strict_dimension_scores = Some(bundle.strict_dimension_scores);
    state.verified_strict_dimension_scores = Some(bundle.verified_strict_dimension_scores);

    save_state(&state, &sp)?;

    println!(
        "  {} → {} {}",
        colorize(&args.finding_id, "cyan"),
        colorize(new_status.as_str(), "green"),
        colorize(&format!("(was {})", old_status.as_str()), "dim"),
    );

    Ok(())
}

// ── Config command ──────────────────────────────────────

fn run_config(args: ConfigArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let cp = config_path(&root);

    match args.action {
        ConfigAction::Get { key } => {
            let config = load_or_default(&cp);
            let json = serde_json::to_value(&config)?;
            match json.get(&key) {
                Some(val) => println!("{key} = {val}"),
                None => println!("{key}: not set (unknown key)"),
            }
        }
        ConfigAction::Set { key, value } => {
            let mut config = load_or_default(&cp);
            autoclimb_config::set_config_value(&mut config, &key, &value)?;
            save_config(&config, &cp)?;
            println!("  {key} = {value}");
        }
        ConfigAction::Unset { key } => {
            let mut config = load_or_default(&cp);
            autoclimb_config::unset_config_value(&mut config, &key)?;
            save_config(&config, &cp)?;
            println!("  {key}: reset to default");
        }
        ConfigAction::List => {
            let config = load_or_default(&cp);
            let json = serde_json::to_value(&config)?;
            if let serde_json::Value::Object(map) = json {
                for (k, v) in &map {
                    println!("  {k} = {v}");
                }
            }
        }
    }

    Ok(())
}

// ── Langs command ──────────────────────────────────────

fn run_langs() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("  {}", colorize("Supported languages:", "bold"));
    println!();

    // Python (custom plugin)
    println!(
        "  {} {}",
        colorize("python", "cyan"),
        colorize("(custom plugin — full depth)", "dim")
    );

    // Generic plugins
    for config in all_builtin_configs() {
        let depth = format!("{:?}", config.depth).to_lowercase();
        let ts = if config.treesitter_lang.is_some() {
            " + tree-sitter"
        } else {
            ""
        };
        println!(
            "  {} {}",
            colorize(&config.name, "cyan"),
            colorize(&format!("({depth}{ts})"), "dim")
        );
    }

    println!();
    println!("  {} languages total", 1 + all_builtin_configs().len());
    Ok(())
}

// ── Review command ─────────────────────────────────────

fn run_review(args: ReviewArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let mut state = load_or_create(&sp)?;

    let requested_review_action = args.prepare
        || args.run_batches
        || args.external_start
        || args.external_submit.is_some()
        || args.import.is_some();
    if requested_review_action {
        require_completed_scan(&state, "review")?;
    }

    if args.prepare || args.run_batches || args.external_start {
        run_review_preflight(&state, args.force_review_rerun)?;
    }

    if args.prepare {
        // Prepare review batches: generate concern analysis
        let config = load_or_default(&config_path(&root));
        let files = review_target_files(&state);
        if files.is_empty() {
            println!("  No open findings to review.");
            println!(
                "  Run `{}` after the next scan or after new issues surface.",
                cli_command("scan")
            );
            return Ok(());
        }

        let concerns = autoclimb_detectors::concerns::generate_concerns(
            &state.findings,
            state.extra.get("lang").and_then(|v| v.as_str()),
        );

        let batch_max = config.review_batch_max_files as usize;
        let batch_count = (files.len() + batch_max - 1) / batch_max.max(1);
        println!("  Prepared {} review batches", batch_count);
        println!("  {} files across {} concerns", files.len(), concerns.len());
        println!(
            "  Run with: {}",
            cli_command(&format!(
                "review --run-batches --backend {} --mode {}",
                args.backend, args.mode
            ))
        );
        return Ok(());
    }

    if args.run_batches {
        let config = load_or_default(&config_path(&root));
        let batch_max = config.review_batch_max_files as usize;

        let files = review_target_files(&state);
        if files.is_empty() {
            println!("  No open findings to review.");
            return Ok(());
        }

        // Generate concerns for prompt context
        let concerns = autoclimb_detectors::concerns::generate_concerns(
            &state.findings,
            state.extra.get("lang").and_then(|v| v.as_str()),
        );

        // Partition files into batches
        let batch_max = batch_max.max(1);
        let batches: Vec<Vec<String>> = files.chunks(batch_max).map(|c| c.to_vec()).collect();
        let total = batches.len();

        println!(
            "  Running {} review batch{} ({} files, backend: {})",
            total,
            if total == 1 { "" } else { "es" },
            files.len(),
            args.backend
        );

        // Build BatchPrompts
        let prompts: Vec<autoclimb_review::types::BatchPrompt> = batches
            .iter()
            .enumerate()
            .map(|(i, batch_files)| {
                let concern_context: String = concerns
                    .iter()
                    .filter(|c| batch_files.contains(&c.file))
                    .map(|c| format!("- [{}] {}: {}", c.concern_type, c.file, c.summary))
                    .collect::<Vec<_>>()
                    .join("\n");

                let prompt = format!(
                    "Review batch {}/{} ({} files).\n\nFiles:\n{}\n\n{}{}",
                    i + 1,
                    total,
                    batch_files.len(),
                    batch_files
                        .iter()
                        .map(|f| format!("- {f}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    if concern_context.is_empty() {
                        ""
                    } else {
                        "Concerns:\n"
                    },
                    concern_context,
                );

                autoclimb_review::types::BatchPrompt {
                    index: i,
                    total,
                    files: batch_files.clone(),
                    prompt,
                }
            })
            .collect();

        let results = match args.backend.as_str() {
            "codex" => run_codex_review_batches(&prompts, &root)?,
            other => {
                return Err(format!(
                    "unsupported review batch backend: {other} (use `--backend codex`; non-Codex reviewers should go through `review --external-start --runner ...`)"
                )
                .into())
            }
        };

        // Report results
        let success_count = results
            .iter()
            .filter(|r| r.status == autoclimb_review::types::BatchStatus::Success)
            .count();
        let total_elapsed: f64 = results.iter().map(|r| r.elapsed_secs).sum();

        println!(
            "  {}/{} batches succeeded ({:.1}s total)",
            success_count, total, total_elapsed
        );

        for r in &results {
            if r.status != autoclimb_review::types::BatchStatus::Success {
                println!(
                    "    Batch {}: {:?} — {}",
                    r.index,
                    r.status,
                    r.raw_output.lines().next().unwrap_or("(no output)")
                );
            }
        }

        if success_count == 0 {
            println!("  No batches succeeded. Check runner configuration.");
            return Ok(());
        }

        // Merge results
        let provenance = autoclimb_review::batch::batch_provenance(&args.backend, None, total);
        let merged = autoclimb_review::merge::merge_batch_results(&results, provenance);

        // Import into state
        let mode = match args.mode.as_str() {
            "trusted" => autoclimb_review::types::ImportMode::TrustedInternal,
            "attested" => autoclimb_review::types::ImportMode::AttestedExternal,
            "findings_only" => autoclimb_review::types::ImportMode::FindingsOnly,
            other => return Err(format!("unknown mode: {other}").into()),
        };
        let import_result = autoclimb_review::import::import_review(&mut state, &merged, mode);

        // Recompute scores and save
        recompute_scores(&mut state);
        save_state(&state, &sp)?;

        println!(
            "  Imported {} findings ({} new, {} updated)",
            import_result.findings_added + import_result.findings_updated,
            import_result.findings_added,
            import_result.findings_updated,
        );
        if import_result.assessments_applied {
            println!("  Subjective assessments applied.");
        } else {
            println!("  Imported findings only; subjective assessments were not applied.");
        }

        return Ok(());
    }

    if args.external_start {
        let session = autoclimb_review::external::start_session(&args.runner, 24);
        let session_json = serde_json::to_string_pretty(&session)?;
        println!("  External review session started:");
        println!("  Session ID: {}", colorize(&session.session_id, "cyan"));
        println!("  Token: {}", session.token);
        println!("  Expires: {}", session.expires_at);

        // Save session to state extra
        state.extra.insert(
            "active_external_session".into(),
            serde_json::to_value(&session)?,
        );
        save_state(&state, &sp)?;

        println!();
        println!("  Submit results with:");
        println!(
            "    {} results.json",
            cli_command(&format!(
                "review --external-submit {} --import",
                session.session_id
            ))
        );
        println!();
        println!("{}", session_json);
        return Ok(());
    }

    if let Some(ref session_id) = args.external_submit {
        // Find the session
        let session_val = state
            .extra
            .get("active_external_session")
            .ok_or("no active external session")?;

        let mut session: autoclimb_review::external::ExternalSession =
            serde_json::from_value(session_val.clone())?;

        if session.session_id != *session_id {
            return Err(format!(
                "session mismatch: expected {}, got {}",
                session.session_id, session_id
            )
            .into());
        }

        // The import path is the next positional-like arg, but we parse it from session_id
        // Actually the user passes it as the --import arg. Let's check:
        let results_path = args
            .import
            .as_ref()
            .ok_or("provide results file with --import <path>")?;

        let payload = autoclimb_review::external::submit_session(&mut session, results_path)?;
        let mode = autoclimb_review::types::ImportMode::AttestedExternal;
        let result = autoclimb_review::import::import_review(&mut state, &payload, mode);

        // Recompute scores
        recompute_scores(&mut state);
        state.extra.insert(
            "active_external_session".into(),
            serde_json::to_value(&session)?,
        );
        save_state(&state, &sp)?;

        println!("  External review submitted.");
        println!(
            "  {} findings added, {} updated",
            result.findings_added, result.findings_updated
        );
        return Ok(());
    }

    if let Some(ref import_path) = args.import {
        let content = std::fs::read_to_string(import_path)?;
        let payload: autoclimb_review::types::ReviewPayload = serde_json::from_str(&content)?;
        let mode = match args.mode.as_str() {
            "trusted" => autoclimb_review::types::ImportMode::TrustedInternal,
            "attested" => autoclimb_review::types::ImportMode::AttestedExternal,
            "findings_only" => autoclimb_review::types::ImportMode::FindingsOnly,
            other => return Err(format!("unknown mode: {other}").into()),
        };

        let result = autoclimb_review::import::import_review(&mut state, &payload, mode);
        recompute_scores(&mut state);
        save_state(&state, &sp)?;

        println!(
            "  Imported {} findings ({} new, {} updated)",
            result.findings_added + result.findings_updated,
            result.findings_added,
            result.findings_updated
        );
        if result.assessments_applied {
            println!("  Subjective assessments applied.");
        } else {
            println!("  Imported findings only; subjective assessments were not applied.");
        }
        return Ok(());
    }

    // Default: show review status
    println!();
    println!("  {}", colorize("Review commands:", "bold"));
    println!("    --prepare           Prepare review batches");
    println!("    --run-batches       Execute review batches");
    println!("    --external-start    Start external review session");
    println!("    --external-submit   Submit external review results");
    println!("    --import <path>     Import review findings from JSON");
    Ok(())
}

fn subjective_assessment_score(assessment: &serde_json::Value, strict: bool) -> Option<f64> {
    if strict {
        assessment
            .get("strict")
            .and_then(|value| value.as_f64())
            .or_else(|| assessment.get("score").and_then(|value| value.as_f64()))
    } else {
        assessment.get("score").and_then(|value| value.as_f64())
    }
}

fn subjective_dimension_entry(
    score: f64,
    configured_weight: f64,
) -> autoclimb_types::scoring::DimensionScoreEntry {
    let mut detectors = BTreeMap::new();
    detectors.insert(
        "subjective_assessment".to_string(),
        serde_json::json!({ "configured_weight": configured_weight }),
    );

    autoclimb_types::scoring::DimensionScoreEntry {
        score,
        tier: 0,
        checks: 1,
        issues: 0,
        detectors,
        extra: BTreeMap::new(),
    }
}

fn apply_subjective_score_overlays(state: &mut autoclimb_types::state::StateModel) {
    if state.subjective_assessments.is_empty() {
        return;
    }

    let registry = autoclimb_review::dimensions::DimensionRegistry::new();

    if let Some(ref mut dims) = state.dimension_scores {
        for (dimension, assessment) in &state.subjective_assessments {
            let Some(score) = subjective_assessment_score(assessment, false) else {
                continue;
            };
            let configured_weight = registry.get(dimension).map(|def| def.weight).unwrap_or(1.0);
            dims.insert(
                dimension.clone(),
                subjective_dimension_entry(score, configured_weight),
            );
        }
        state.overall_score = autoclimb_scoring::results::compute_health_score(dims);
    }

    if let Some(ref mut strict_dims) = state.strict_dimension_scores {
        for (dimension, assessment) in &state.subjective_assessments {
            let Some(score) = subjective_assessment_score(assessment, true) else {
                continue;
            };
            let configured_weight = registry.get(dimension).map(|def| def.weight).unwrap_or(1.0);
            strict_dims.insert(
                dimension.clone(),
                subjective_dimension_entry(score, configured_weight),
            );
        }
        state.strict_score = autoclimb_scoring::results::compute_health_score(strict_dims);
    }

    if let Some(ref mut verified_dims) = state.verified_strict_dimension_scores {
        for (dimension, assessment) in &state.subjective_assessments {
            let Some(score) = subjective_assessment_score(assessment, true) else {
                continue;
            };
            let configured_weight = registry.get(dimension).map(|def| def.weight).unwrap_or(1.0);
            verified_dims.insert(
                dimension.clone(),
                subjective_dimension_entry(score, configured_weight),
            );
        }
        state.verified_strict_score =
            autoclimb_scoring::results::compute_health_score(verified_dims);
    }
}

fn recompute_scores(state: &mut autoclimb_types::state::StateModel) {
    let potentials: BTreeMap<String, u64> = state
        .potentials
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
        .collect();
    let bundle = autoclimb_scoring::results::compute_score_bundle(&state.findings, &potentials);
    state.overall_score = bundle.overall_score;
    state.objective_score = bundle.objective_score;
    state.strict_score = bundle.strict_score;
    state.verified_strict_score = bundle.verified_strict_score;
    state.dimension_scores = Some(bundle.dimension_scores);
    state.strict_dimension_scores = Some(bundle.strict_dimension_scores);
    state.verified_strict_dimension_scores = Some(bundle.verified_strict_dimension_scores);
    apply_subjective_score_overlays(state);
}

// ── Fix command ────────────────────────────────────────

fn run_fix(args: FixArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let state = load_or_create(&sp)?;

    // Filter fixable findings (AutoFix tier)
    let fixable: Vec<&Finding> = state
        .findings
        .values()
        .filter(|f| {
            if f.status != Status::Open || f.suppressed {
                return false;
            }
            if f.tier != autoclimb_types::enums::Tier::AutoFix {
                return false;
            }
            if let Some(ref det) = args.detector {
                if f.detector != *det {
                    return false;
                }
            }
            if let Some(ref pat) = args.pattern {
                let pat_lower = pat.to_lowercase();
                if !f.summary.to_lowercase().contains(&pat_lower)
                    && !f.file.to_lowercase().contains(&pat_lower)
                {
                    return false;
                }
            }
            true
        })
        .collect();

    if fixable.is_empty() {
        println!("  No auto-fixable findings match the given filters.");
        return Ok(());
    }

    if args.dry_run {
        println!("  {} auto-fixable findings (dry run):", fixable.len());
        for f in &fixable {
            println!(
                "    {} {} {}",
                colorize(&f.detector, "cyan"),
                f.file,
                f.summary
            );
        }
        return Ok(());
    }

    println!("  {} auto-fixable findings:", fixable.len());
    for f in &fixable {
        println!(
            "    {} {}",
            colorize(&f.detector, "cyan"),
            colorize(&f.summary, "dim")
        );
    }
    println!();
    println!(
        "  {}",
        colorize("Fix execution requires detector-specific fixers.", "dim")
    );
    println!(
        "  {}",
        colorize(
            "Run external tools directly: ruff check --fix, eslint --fix, etc.",
            "dim"
        )
    );

    Ok(())
}

// ── Move command ────────────────────────────────────────

fn run_move(args: MoveArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);

    let src = root.join(&args.source);
    let dst = root.join(&args.dest);

    if !src.exists() {
        return Err(format!("source not found: {}", src.display()).into());
    }

    if args.dry_run {
        println!("  Dry run — would move:");
        println!(
            "    {} → {}",
            colorize(&args.source, "cyan"),
            colorize(&args.dest, "green")
        );

        // Count findings that reference the source path
        let state = load_or_create(&sp)?;
        let affected: Vec<&str> = state
            .findings
            .values()
            .filter(|f| f.file == args.source || f.file.starts_with(&format!("{}/", args.source)))
            .map(|f| f.file.as_str())
            .collect();

        if !affected.is_empty() {
            println!(
                "    {} findings would be updated",
                colorize(&affected.len().to_string(), "yellow")
            );
        }
        return Ok(());
    }

    // Create destination parent if needed
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::rename(&src, &dst)?;
    println!(
        "  Moved {} → {}",
        colorize(&args.source, "cyan"),
        colorize(&args.dest, "green")
    );

    // Update state: rewrite finding IDs and file references
    let mut state = load_or_create(&sp)?;
    let mut updated_count = 0u32;

    let old_findings: Vec<(String, Finding)> = state
        .findings
        .iter()
        .filter(|(_, f)| f.file == args.source || f.file.starts_with(&format!("{}/", args.source)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    for (old_id, mut finding) in old_findings {
        let new_file = if finding.file == args.source {
            args.dest.clone()
        } else {
            finding.file.replacen(&args.source, &args.dest, 1)
        };

        let new_id = old_id.replacen(&finding.file, &new_file, 1);

        finding.file = new_file;
        state.findings.remove(&old_id);
        state.findings.insert(new_id, finding);
        updated_count += 1;
    }

    save_state(&state, &sp)?;

    if updated_count > 0 {
        println!(
            "  Updated {} finding references",
            colorize(&updated_count.to_string(), "yellow")
        );
    }

    Ok(())
}

// ── Queue command ──────────────────────────────────────

fn run_queue(args: QueueArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let state = load_or_create(&sp)?;

    // Load plan if present
    let plan: Option<PlanModel> = state
        .plan
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let options = QueueBuildOptions {
        tier: args.tier,
        count: args.count,
        scope: args.scope,
        chronic: args.chronic,
        ..Default::default()
    };

    let queue = build_queue(&state.findings, plan.as_ref(), &options);

    if args.json {
        let items: Vec<serde_json::Value> = queue
            .iter()
            .enumerate()
            .map(|(i, item)| {
                serde_json::json!({
                    "rank": i + 1,
                    "finding_id": item.finding_id,
                    "file": item.file,
                    "detector": item.detector,
                    "tier": item.tier.as_u8(),
                    "summary": item.summary,
                    "reopen_count": item.reopen_count,
                    "is_cluster": item.is_cluster,
                    "is_skipped": item.is_skipped,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if queue.is_empty() {
        println!(
            "  No items in queue. Run {} first.",
            colorize(&cli_command("scan"), "cyan")
        );
        return Ok(());
    }

    println!(
        "\n  {} Work Queue ({} items)\n",
        colorize("▸", "cyan"),
        queue.len()
    );

    for (i, item) in queue.iter().enumerate() {
        let tier_str = format!("T{}", item.tier.as_u8());
        let skip_marker = if item.is_skipped { " [skipped]" } else { "" };
        let cluster_marker = if item.is_cluster { " [cluster]" } else { "" };
        let reopen = if item.reopen_count > 0 {
            format!(" ({}x reopened)", item.reopen_count)
        } else {
            String::new()
        };

        let markers = format!("{skip_marker}{cluster_marker}");
        println!(
            "  {:>3}. {} {} {}{}{}\n       {}",
            i + 1,
            colorize(
                &tier_str,
                if item.tier.as_u8() <= 2 {
                    "green"
                } else {
                    "yellow"
                }
            ),
            colorize(&item.detector, "cyan"),
            item.file,
            reopen,
            markers,
            item.summary,
        );
    }

    println!();
    Ok(())
}

// ── Plan command ───────────────────────────────────────

fn run_plan(args: PlanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let mut state = load_or_create(&sp)?;
    require_completed_scan(&state, "plan")?;

    let mut plan: PlanModel = state
        .plan
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(PlanModel::empty);

    match args.action {
        PlanAction::Show => {
            println!("\n  {} Living Plan\n", colorize("▸", "cyan"));
            println!("  Queue order: {} items", plan.queue_order.len());
            println!("  Skipped:     {} items", plan.skipped.len());
            println!("  Clusters:    {} groups", plan.clusters.len());
            println!("  Superseded:  {} entries", plan.superseded.len());

            if !plan.queue_order.is_empty() {
                println!("\n  Queue order (first 10):");
                for (i, id) in plan.queue_order.iter().take(10).enumerate() {
                    let summary = state
                        .findings
                        .get(id)
                        .map(|f| f.summary.as_str())
                        .unwrap_or("(not found)");
                    println!("    {:>3}. {} — {}", i + 1, colorize(id, "cyan"), summary);
                }
            }

            if !plan.skipped.is_empty() {
                println!("\n  Skipped items:");
                for (id, entry) in &plan.skipped {
                    let kind = format!("{:?}", entry.kind);
                    let reason = entry.reason.as_deref().unwrap_or("");
                    println!("    {} ({}) {}", colorize(id, "yellow"), kind, reason);
                }
            }

            println!();
        }
        PlanAction::Skip(skip_args) => {
            let kind = if skip_args.permanent {
                autoclimb_plan::plan_model::SkipKind::Permanent
            } else {
                autoclimb_plan::plan_model::SkipKind::Temporary
            };

            autoclimb_plan::operations::skip_items(
                &mut plan,
                &skip_args.finding_ids,
                kind,
                skip_args.reason,
                None,
                state.scan_count,
            );

            state.plan = Some(serde_json::to_value(&plan)?);
            save_state(&state, &sp)?;

            println!(
                "  Skipped {} item(s)",
                colorize(&skip_args.finding_ids.len().to_string(), "yellow")
            );
        }
        PlanAction::Unskip(unskip_args) => {
            autoclimb_plan::operations::unskip_items(&mut plan, &unskip_args.finding_ids, true);

            state.plan = Some(serde_json::to_value(&plan)?);
            save_state(&state, &sp)?;

            println!(
                "  Unskipped {} item(s)",
                colorize(&unskip_args.finding_ids.len().to_string(), "green")
            );
        }
        PlanAction::Move(move_args) => {
            autoclimb_plan::operations::move_items(&mut plan, &move_args.finding_ids, 0);

            state.plan = Some(serde_json::to_value(&plan)?);
            save_state(&state, &sp)?;

            println!(
                "  Moved {} item(s) to front of queue",
                colorize(&move_args.finding_ids.len().to_string(), "cyan")
            );
        }
        PlanAction::Reset => {
            plan = PlanModel::empty();
            state.plan = Some(serde_json::to_value(&plan)?);
            save_state(&state, &sp)?;

            println!("  Plan reset to empty");
        }
    }

    Ok(())
}

// ── Tree ───────────────────────────────────────────────

fn run_tree(args: TreeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let state = load_or_create(&sp)?;

    let files: Vec<String> = state
        .findings
        .keys()
        .map(|k| state.findings[k].file.clone())
        .collect();

    // Deduplicate files
    let mut unique_files: Vec<String> = files
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    unique_files.sort();

    let tree = autoclimb_output::tree::render_tree(&unique_files, &state.findings);
    println!("{tree}");

    Ok(())
}

// ── Viz ────────────────────────────────────────────────

fn run_viz(args: VizArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let sp = state_path(&root);
    let state = load_or_create(&sp)?;

    let files: Vec<String> = state
        .findings
        .values()
        .map(|f| f.file.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let scores = serde_json::json!({
        "overall": state.overall_score,
        "objective": state.objective_score,
        "strict": state.strict_score,
    });

    autoclimb_output::visualize::generate_html_report(
        &state.findings,
        &files,
        &scores,
        &args.output,
    )?;

    println!(
        "  {} generated: {}",
        colorize("Report", "bold_green"),
        args.output.display()
    );

    Ok(())
}

// ── Detect ─────────────────────────────────────────────

fn run_detect(args: DetectArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);

    // Find the detector
    let detector_info = detector_by_name(&args.name);
    if detector_info.is_none() {
        eprintln!("Unknown detector: {}", args.name);
        eprintln!("Available detectors:");
        for d in DETECTORS {
            eprintln!("  {} — {}", d.name, d.display);
        }
        std::process::exit(1);
    }

    // Detect language
    let lang = args.lang.unwrap_or_else(|| {
        if detect_python_project(&root) {
            "python".to_string()
        } else {
            // Try generic detection
            for config in all_builtin_configs() {
                if detect_project(&root, &config) {
                    return config.name.clone();
                }
            }
            "unknown".to_string()
        }
    });

    // Build context and run detector
    let (files, ctx) = build_context_for_lang(&root, &lang, &[]);
    if files.is_empty() {
        println!("No {} files found", lang);
        return Ok(());
    }

    // Find matching detector phase
    let phases = get_detector_phases(&lang);
    let matching: Vec<&dyn DetectorPhase> = phases
        .iter()
        .filter(|p| p.label().contains(&args.name) || args.name.contains(p.label()))
        .map(|p| p.as_ref())
        .collect();

    if matching.is_empty() {
        println!(
            "Detector '{}' not available for language '{}'",
            args.name, lang
        );
        return Ok(());
    }

    let mut total_findings = 0;
    for phase in matching {
        println!("  Running: {}", colorize(phase.label(), "cyan"));
        match phase.run(&root, &ctx) {
            Ok(output) => {
                if args.json {
                    println!("{}", serde_json::to_string_pretty(&output.findings)?);
                } else {
                    for finding in &output.findings {
                        println!("    {}", format_finding(finding, false));
                    }
                    total_findings += output.findings.len();
                }
            }
            Err(e) => {
                eprintln!("  Error: {e}");
            }
        }
    }

    if !args.json {
        println!(
            "\n  {} findings from detector '{}'",
            total_findings, args.name
        );
    }

    Ok(())
}

/// Build scan context for a language.
fn build_context_for_lang(
    root: &std::path::Path,
    lang: &str,
    exclude: &[String],
) -> (Vec<String>, autoclimb_detectors::context::ScanContext) {
    use autoclimb_detectors::context::ScanContext;

    if lang == "python" {
        let plugin = PythonPlugin;
        let files = plugin.discover_files(root, exclude);
        let ctx = plugin.build_context(root, files.clone(), exclude.to_vec());
        (files, ctx)
    } else {
        for config in all_builtin_configs() {
            if config.name == lang {
                let files = config.discover_files(root, exclude);
                let ctx = config.build_context(root, files.clone(), exclude.to_vec());
                return (files, ctx);
            }
        }
        (
            vec![],
            ScanContext {
                lang_name: lang.into(),
                files: vec![],
                dep_graph: None,
                zone_map: autoclimb_discovery::zones::ZoneMap::new(&[], &[]),
                exclusions: vec![],
                entry_patterns: vec![],
                barrel_names: std::collections::BTreeSet::new(),
                large_threshold: 300,
                complexity_threshold: 20,
            },
        )
    }
}

/// Get detector phases for a language.
fn get_detector_phases(lang: &str) -> Vec<Box<dyn DetectorPhase>> {
    if lang == "python" {
        PythonPlugin.phases()
    } else {
        for config in all_builtin_configs() {
            if config.name == lang {
                return config.phases();
            }
        }
        vec![]
    }
}

// ── Exclude ────────────────────────────────────────────

fn run_exclude(args: ExcludeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root = resolve_root(args.path);
    let cp = config_path(&root);
    let mut config = load_or_default(&cp);

    match args.action {
        ExcludeAction::List => {
            if config.exclude.is_empty() {
                println!("  No exclusion patterns configured");
            } else {
                println!("  Exclusion patterns:");
                for pat in &config.exclude {
                    println!("    - {pat}");
                }
            }
        }
        ExcludeAction::Add { pattern } => {
            if config.exclude.contains(&pattern) {
                println!("  Pattern already exists: {pattern}");
            } else {
                config.exclude.push(pattern.clone());
                save_config(&config, &cp)?;
                println!("  Added: {}", colorize(&pattern, "green"));
            }
        }
        ExcludeAction::Remove { pattern } => {
            let before = config.exclude.len();
            config.exclude.retain(|p| p != &pattern);
            if config.exclude.len() < before {
                save_config(&config, &cp)?;
                println!("  Removed: {}", colorize(&pattern, "red"));
            } else {
                println!("  Pattern not found: {pattern}");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recompute_scores_applies_subjective_assessments() {
        let mut state = autoclimb_types::state::StateModel::empty();
        state.subjective_assessments.insert(
            "design_coherence".into(),
            serde_json::json!({
                "score": 82.0,
                "strict": 79.0,
                "source": "codex",
                "assessed_at": "2026-03-07T00:00:00Z",
                "placeholder": false,
                "provisional_override": false,
                "integrity_penalty": serde_json::Value::Null,
            }),
        );

        recompute_scores(&mut state);

        assert_eq!(state.objective_score, 100.0);
        assert_eq!(state.overall_score, 82.0);
        assert_eq!(state.strict_score, 79.0);
        assert_eq!(state.verified_strict_score, 79.0);
        assert!(state
            .dimension_scores
            .as_ref()
            .and_then(|dims| dims.get("design_coherence"))
            .and_then(|entry| entry.detectors.get("subjective_assessment"))
            .is_some());
    }
}
