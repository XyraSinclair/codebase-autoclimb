//! Hardcoded dimension definitions.
//!
//! Ported from Python: intelligence/review/dimensions/dimensions.json

use super::DimensionDef;

pub(super) fn all_dimensions() -> Vec<DimensionDef> {
    vec![
        DimensionDef {
            key: "naming_quality",
            display_name: "Naming quality",
            description: "Function/variable/file names that communicate intent",
            look_for: "\
- Generic verbs that reveal nothing: process, handle, do, run, manage
- Name/behavior mismatch: getX() that mutates state, isX() returning non-boolean
- Vocabulary divergence from codebase norms
- Abbreviations inconsistent with codebase conventions",
            skip: "\
- Standard framework names (render, mount, useEffect)
- Short-lived loop variables (i, j, k)
- Well-known abbreviations matching codebase convention (ctx, req, res)",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "logic_clarity",
            display_name: "Logic clarity",
            description: "Control flow and logic that provably does what it claims",
            look_for: "\
- Identical if/else or ternary branches (same code on both sides)
- Dead code paths: code after unconditional return/raise/throw/break
- Always-true or always-false conditions
- Redundant null/undefined checks on values that cannot be null
- Async functions that never await
- Boolean expressions that simplify: if x: return True else: return False",
            skip: "\
- Deliberate no-op branches with explanatory comments
- Framework lifecycle methods that must be async by contract
- Guard clauses that are defensive by design",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "type_safety",
            display_name: "Type safety",
            description: "Type annotations that match runtime behavior",
            look_for: "\
- Return type annotations that don't cover all code paths
- Parameters typed as X but called with Y
- Union types that could be narrowed
- Missing annotations on public API functions
- Type: ignore comments without explanation
- TypedDict fields marked Required but accessed via .get() with defaults",
            skip: "\
- Untyped private helpers in well-typed modules
- Dynamic framework code where typing is impractical
- Test code with loose typing",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "contract_coherence",
            display_name: "Contract coherence",
            description: "Functions and modules that honor their stated contracts",
            look_for: "\
- Return type annotation lies: declared type doesn't match all return paths
- Docstring/signature divergence: params described in docs but not in function
- Functions named getX that mutate state
- Module-level API inconsistency: some exports follow a pattern, one doesn't
- Error contracts: function says it throws but silently returns None",
            skip: "\
- Protocol/interface stubs (abstract methods with placeholder returns)
- Test helpers where loose typing is intentional
- Overloaded functions with multiple valid return types",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "error_consistency",
            display_name: "Error consistency",
            description: "Consistent error strategies, preserved context, predictable failure modes",
            look_for: "\
- Mixed error strategies: some functions throw, others return null, others use Result
- Error context lost at boundaries: catch-and-rethrow without wrapping original
- Inconsistent error types: custom error classes in some modules, bare strings in others
- Silent error swallowing: catches that log but don't propagate or recover
- Missing error handling on I/O boundaries",
            skip: "\
- Intentional error boundaries at top-level handlers
- Different strategies for different layers (Result in core, throw in CLI)",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "Use evidence from holistic_context.errors.exception_hotspots. \
Investigate whether error handling is designed or accidental. \
Check for broad catches masking specific failure modes.",
        },
        DimensionDef {
            key: "abstraction_fitness",
            display_name: "Abstraction fitness",
            description: "Abstractions that pay for themselves with real leverage",
            look_for: "\
- Pass-through wrappers or interfaces that add no behavior, policy, or translation
- Cross-cutting wrapper chains where call depth increases without added value
- Interface/protocol families where most declared contracts have only one implementation
- Systemic util/helper dumping grounds that create low cohesion
- Leaky abstractions: callers consistently bypass intended interfaces
- Wide options/context bag APIs that hide true domain boundaries
- Generic/type-parameter machinery used in only one concrete way
- Delegation-heavy classes where most methods forward to an inner object
- Facade/re-export modules that define no logic of their own",
            skip: "\
- Dependency-injection or framework abstractions required for wiring/testability
- Adapters that intentionally isolate external API volatility
- Cases where abstraction clearly reduces duplication across multiple callers
- Thin wrappers that consistently enforce policy (auth/logging/metrics/caching)
- If the core issue is dependency direction or cycles, use cross_module_architecture",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "Use evidence from holistic_context.abstractions: \
delegation_heavy_classes, facade_modules, typed_dict_violations, complexity_hotspots. \
Include delegation_density, definition_directness, type_discipline alongside \
existing sub-axes in dimension_notes when evidence supports it.",
        },
        DimensionDef {
            key: "ai_generated_debt",
            display_name: "AI-generated debt",
            description: "LLM-hallmark patterns: restating comments, defensive overengineering, boilerplate",
            look_for: "\
- Restating comments that echo the code without adding insight
- Nosy debug logging: entry/exit logs on every function, full object dumps
- Defensive overengineering: null checks on non-nullable values, try-catch around pure expressions
- Docstring bloat: multi-line docstrings on trivial 2-line functions
- Pass-through wrapper functions with no added logic
- Generic names in domain code: handleData, processItem, doOperation
- Identical boilerplate error handling copied verbatim across files",
            skip: "\
- Comments explaining WHY (business rules, non-obvious constraints)
- Defensive checks at genuine API boundaries (user input, network, file I/O)
- Generated code (protobuf, GraphQL codegen, ORM migrations)
- Wrapper functions that add auth, logging, metrics, or caching",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "high_level_elegance",
            display_name: "High-level elegance",
            description: "Clear decomposition, coherent ownership, domain-aligned structure",
            look_for: "\
- Top-level packages/files map to domain capabilities rather than historical accidents
- Ownership and change boundaries are predictable
- Public surface (exports/entry points) is small and consistent with stated responsibility
- Project contracts and reference docs match runtime reality
- Subsystem decomposition localizes change without surprising ripple edits
- A small set of architectural patterns is used consistently",
            skip: "\
- When dependency direction/cycle/hub failures are PRIMARY, use cross_module_architecture
- When handoff mechanics are PRIMARY, use mid_level_elegance
- When function/class internals are PRIMARY, use low_level_elegance or logic_clarity
- Pure naming/style nits with no impact on role clarity",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "mid_level_elegance",
            display_name: "Mid-level elegance",
            description: "Quality of handoffs and integration seams across modules and layers",
            look_for: "\
- Inputs/outputs across boundaries are explicit, minimal, and unsurprising
- Data translation at boundaries happens in one obvious place
- Error and lifecycle propagation across boundaries follows predictable patterns
- Orchestration reads as composition of collaborators, not tangled back-and-forth
- Integration seams avoid glue-code entropy",
            skip: "\
- When top-level decomposition is PRIMARY, use high_level_elegance
- When function/class internals are PRIMARY, use low_level_elegance
- Pure API/type contract defects (belongs to contract_coherence)
- Standalone naming/style preferences that do not affect handoffs",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "low_level_elegance",
            display_name: "Low-level elegance",
            description: "Direct, precise function and class internals",
            look_for: "\
- Control flow is direct and intention-revealing; branches are necessary and distinct
- State mutation and side effects are explicit, local, and bounded
- Edge-case handling is precise without defensive sprawl
- Extraction level is balanced: avoids both monoliths and micro-fragmentation
- Helper extraction style is consistent across related modules",
            skip: "\
- When file responsibility/package role is PRIMARY, use high_level_elegance
- When inter-module seam choreography is PRIMARY, use mid_level_elegance
- When dependency topology is PRIMARY, use cross_module_architecture
- Provable logic/type/error defects already captured by other dimensions",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "cross_module_architecture",
            display_name: "Cross-module architecture",
            description: "Dependency direction, cycles, hub modules, and boundary integrity",
            look_for: "\
- Layer/dependency direction violations repeated across multiple modules
- Cycles or hub modules that create large blast radius for common changes
- Documented architecture contracts drifting from runtime
- Cross-module coordination through shared mutable state or import-time side effects
- Compatibility shim paths that persist without active need and blur boundaries
- Cross-package duplication indicating a missing shared boundary",
            skip: "\
- Intentional facades/re-exports with clear API purpose
- Framework-required patterns (Django settings, plugin registries)
- Package naming/placement tidy-ups (belongs to package_organization)
- Local readability/craft issues (belongs to low_level_elegance)",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "Consult holistic_context.coupling.boundary_violations for \
import paths that cross architectural boundaries, and \
holistic_context.dependencies.deferred_import_density for files with many \
function-level imports (proxy for cycle pressure).",
        },
        DimensionDef {
            key: "initialization_coupling",
            display_name: "Initialization coupling",
            description: "Boot-order dependencies, import-time side effects, global singletons",
            look_for: "\
- Module-level code that depends on another module having been imported first
- Import-time side effects: DB connections, file I/O, network calls at module scope
- Global singletons where creation order matters across modules
- Environment variable reads at import time (fragile in testing)
- Circular init dependencies hidden behind conditional or lazy imports",
            skip: "\
- Standard library initialization (logging.basicConfig)
- Framework bootstrap (app.configure, server.listen)",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "Use evidence from holistic_context.scan_evidence.mutable_globals \
and holistic_context.errors.mutable_globals. Investigate initialization ordering \
dependencies, coupling through shared mutable state, whether state should be \
encapsulated behind proper registry/context manager.",
        },
        DimensionDef {
            key: "convention_outlier",
            display_name: "Convention outlier",
            description: "Naming convention drift, inconsistent file organization, style islands",
            look_for: "\
- Naming convention drift: snake_case in a camelCase codebase or vice versa
- Inconsistent file organization: some dirs use index files, others don't
- Mixed export patterns across sibling modules
- Style islands: one directory uses a completely different pattern
- Sibling modules following different behavioral protocols
- Inconsistent plugin organization
- Large __init__.py re-export surfaces that obscure internal module structure",
            skip: "\
- Intentional variation for different module types (config vs logic)
- Third-party code or generated files following their own conventions",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "Consult holistic_context.conventions.duplicate_clusters for \
cross-file function duplication and conventions.naming_drift for directory-level \
naming inconsistency.",
        },
        DimensionDef {
            key: "dependency_health",
            display_name: "Dependency health",
            description: "Unused deps, version conflicts, multiple libs for same purpose, heavy deps",
            look_for: "\
- Multiple libraries for the same purpose (moment + dayjs, axios + fetch wrapper)
- Heavy dependencies pulled in for light use (lodash for one function)
- Circular dependency cycles visible in the import graph
- Unused dependencies in package.json/requirements.txt
- Version conflicts or pinning issues visible in lock files",
            skip: "\
- Dev dependencies (test, build, lint tools)
- Peer dependencies required by frameworks",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "test_strategy",
            display_name: "Test strategy",
            description: "Untested critical paths, coupling, snapshot overuse, fragility patterns",
            look_for: "\
- Critical paths with zero test coverage (high-importer files, core business logic)
- Test-production coupling: tests that break when implementation details change
- Snapshot test overuse: >50% of tests are snapshot-based
- Missing integration tests: unit tests exist but no cross-module verification
- Test fragility: tests that depend on timing, ordering, or external state",
            skip: "\
- Low-value files intentionally untested (types, constants, index files)
- Generated code that shouldn't have custom tests",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "api_surface_coherence",
            display_name: "API surface coherence",
            description: "Inconsistent API shapes, mixed sync/async, overloaded interfaces",
            look_for: "\
- Inconsistent API shapes: similar functions with different parameter ordering
- Mixed sync/async in the same module's public API
- Overloaded interfaces: one function doing too many things based on argument types
- Missing error contracts: no documentation or types indicating what can fail
- Public functions with >5 parameters",
            skip: "\
- Internal/private APIs where flexibility is acceptable
- Framework-imposed patterns (React hooks must follow rules of hooks)",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "authorization_consistency",
            display_name: "Authorization consistency",
            description: "Auth/permission patterns consistently applied across the codebase",
            look_for: "\
- Route handlers with auth decorators/middleware on some siblings but not others
- RLS enabled on some tables but not siblings in the same domain
- Permission strings as magic literals instead of shared constants
- Mixed trust boundaries: some endpoints validate user input, siblings don't
- Service role / admin bypass without audit logging or access control",
            skip: "\
- Public routes explicitly documented as unauthenticated (health checks, login)
- Internal service-to-service calls behind network-level auth
- Dev/test endpoints behind feature flags or environment checks",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "incomplete_migration",
            display_name: "Incomplete migration",
            description: "Old+new API coexistence, deprecated-but-called symbols, stale migration shims",
            look_for: "\
- Old and new API patterns coexisting: class+functional components, axios+fetch
- Deprecated symbols still called by active code
- Compatibility shims that no caller actually needs anymore
- Mixed JS/TS files for the same module (incomplete TypeScript migration)
- Stale migration TODOs referencing migrate, legacy, old api, remove after",
            skip: "\
- Active, intentional migrations with tracked progress
- Backward-compatibility for external consumers (published APIs, libraries)
- Gradual rollouts behind feature flags with clear ownership",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "package_organization",
            display_name: "Package organization",
            description: "Directory layout quality and navigability",
            look_for: "\
- Straggler roots: root-level files with low fan-in (<5 importers) that should move
- Import-affinity mismatch: file imports mostly from one sibling domain but lives outside
- Coupling-direction failures: reciprocal/bidirectional directory edges
- Flat directory overload: >10 files with mixed concerns
- Ambiguous folder naming: directory names don't reflect contained responsibilities",
            skip: "\
- Root-level files that ARE genuinely core (high fan-in >= 5, imported across subdirectories)
- Small projects (<20 files) where flat structure is appropriate
- Framework-imposed directory layouts (src/, lib/, dist/)
- Test directories mirroring production structure
- Aesthetic preferences without measurable impact",
            enabled_by_default: true,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "Ground scoring in objective structure signals from \
holistic_context.structure (root_files fan_in/fan_out roles, directory_profiles, \
coupling_matrix). Prefer thresholded evidence. Also consult \
holistic_context.structure.flat_dir_findings.",
        },
        DimensionDef {
            key: "design_coherence",
            display_name: "Design coherence",
            description: "Are structural design decisions sound — functions focused, abstractions earned, patterns consistent?",
            look_for: "\
- Functions doing too many things — multiple distinct responsibilities in one body
- Parameter lists that should be config/context objects
- Files accumulating issues across many dimensions — likely mixing unrelated concerns
- Deep nesting that could be flattened with early returns or extraction
- Repeated structural patterns that should be data-driven",
            skip: "\
- Functions that are long but have a single coherent responsibility
- Parameter lists where grouping would obscure meaning
- Files that are large because their domain is genuinely complex
- Nesting that is inherent to the problem (recursive tree processing)",
            enabled_by_default: true,
            weight: 10.0,
            reset_on_scan: true,
            evidence_focus: "Use evidence from holistic_context.scan_evidence.signal_density — \
files where multiple mechanical detectors fired. Investigate what design change \
would address multiple signals simultaneously. Check scan_evidence.complexity_hotspots \
for files with high responsibility cluster counts.",
        },
        // ── Non-default dimensions ────────────────────────────
        DimensionDef {
            key: "comment_quality",
            display_name: "Comment quality",
            description: "Comments that add value vs mislead or waste space",
            look_for: "\
- Stale comments describing behavior the code no longer implements
- Restating comments (// increment i above i += 1)
- Missing comments on complex/non-obvious code (regex, algorithms, business rules)
- Docstring/signature divergence (params in docs not in function)
- TODOs without issue references or dates",
            skip: "\
- Section dividers and organizational comments
- License headers
- Type annotations that serve as documentation",
            enabled_by_default: false,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
        DimensionDef {
            key: "authorization_coherence",
            display_name: "Authorization coherence",
            description: "Auth/validation consistency within a single file",
            look_for: "\
- Auth decorators/middleware on some route handlers but not sibling handlers in same file
- Permission strings as magic literals instead of constants or enums
- Input validation on some parameters but not sibling parameters of same type
- Mixed auth strategies in the same router (session + token + API key)
- Service role / admin bypass without audit logging",
            skip: "\
- Files with only public/unauthenticated endpoints
- Internal utility modules that don't handle requests
- Modules with <20 LOC (insufficient code to evaluate auth patterns)",
            enabled_by_default: false,
            weight: 1.0,
            reset_on_scan: false,
            evidence_focus: "",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_dimensions_have_unique_keys() {
        let dims = all_dimensions();
        let mut seen = std::collections::HashSet::new();
        for d in &dims {
            assert!(seen.insert(d.key), "duplicate dimension key: {}", d.key);
        }
    }

    #[test]
    fn all_dimensions_have_content() {
        for d in all_dimensions() {
            assert!(!d.description.is_empty(), "{} has empty description", d.key);
            assert!(!d.look_for.is_empty(), "{} has empty look_for", d.key);
            assert!(!d.skip.is_empty(), "{} has empty skip", d.key);
        }
    }

    #[test]
    fn default_count() {
        let defaults: Vec<_> = all_dimensions()
            .into_iter()
            .filter(|d| d.enabled_by_default)
            .collect();
        assert_eq!(defaults.len(), 20, "should have 20 default dimensions");
    }
}
