//! Context size budgeting.
//!
//! Ensures the holistic context fits within LLM context window limits.

use super::HolisticContext;

/// Default context budget in characters.
pub const DEFAULT_BUDGET_CHARS: usize = 50_000;

/// Trim holistic context to fit within a character budget.
///
/// Progressively removes lower-priority fields until the
/// serialized JSON fits within the budget.
pub fn trim_to_budget(ctx: &mut HolisticContext, budget_chars: usize) {
    // Phase 1: Trim lists to reasonable maxima
    ctx.scan_evidence.complexity_hotspots.truncate(10);
    ctx.scan_evidence.exception_hotspots.truncate(10);
    ctx.scan_evidence.signal_density.truncate(15);
    ctx.scan_evidence.boundary_violations.truncate(10);
    ctx.scan_evidence.mutable_globals.truncate(10);
    ctx.coupling.high_fan_in.truncate(10);
    ctx.coupling.high_fan_out.truncate(10);
    ctx.conventions.duplicate_clusters.truncate(10);
    ctx.conventions.naming_drift.truncate(10);
    ctx.abstractions.delegation_heavy_classes.truncate(10);
    ctx.abstractions.facade_modules.truncate(10);
    ctx.structure.root_files.truncate(20);
    ctx.structure.directory_profiles.truncate(15);

    if estimate_size(ctx) <= budget_chars {
        return;
    }

    // Phase 2: Aggressively trim
    ctx.scan_evidence.complexity_hotspots.truncate(5);
    ctx.scan_evidence.signal_density.truncate(8);
    ctx.scan_evidence.boundary_violations.truncate(5);
    ctx.coupling.high_fan_in.truncate(5);
    ctx.conventions.duplicate_clusters.truncate(5);
    ctx.structure.directory_profiles.truncate(8);
    ctx.structure.coupling_matrix.clear();

    if estimate_size(ctx) <= budget_chars {
        return;
    }

    // Phase 3: Remove optional sections entirely
    ctx.abstractions = super::AbstractionContext::default();
    ctx.structure = super::StructureContext::default();
    ctx.dependencies = super::DependencyContext::default();
}

/// Estimate serialized JSON size.
fn estimate_size(ctx: &HolisticContext) -> usize {
    // Quick estimate: serialize and measure
    serde_json::to_string(ctx).map(|s| s.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_context_fits_budget() {
        let mut ctx = HolisticContext {
            scan_evidence: Default::default(),
            coupling: Default::default(),
            dependencies: Default::default(),
            conventions: Default::default(),
            errors: Default::default(),
            abstractions: Default::default(),
            structure: Default::default(),
        };

        trim_to_budget(&mut ctx, DEFAULT_BUDGET_CHARS);
        let size = estimate_size(&ctx);
        assert!(size < DEFAULT_BUDGET_CHARS);
    }

    #[test]
    fn large_context_trimmed() {
        let mut ctx = HolisticContext {
            scan_evidence: Default::default(),
            coupling: Default::default(),
            dependencies: Default::default(),
            conventions: Default::default(),
            errors: Default::default(),
            abstractions: Default::default(),
            structure: Default::default(),
        };

        // Add many entries
        for i in 0..100 {
            ctx.scan_evidence
                .complexity_hotspots
                .push(super::super::Hotspot {
                    file: format!("src/file{i}.py"),
                    score: i as f64,
                    detail: "x".repeat(200),
                });
        }

        trim_to_budget(&mut ctx, 5000);
        assert!(ctx.scan_evidence.complexity_hotspots.len() <= 10);
    }
}
