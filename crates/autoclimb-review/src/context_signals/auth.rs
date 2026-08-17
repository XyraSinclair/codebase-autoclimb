//! Auth coverage signal detector.
//!
//! Detects authentication/authorization coverage patterns:
//! - Route handlers without auth decorators/middleware
//! - Service role usage in client code
//! - Missing RLS (Row Level Security) patterns

use super::{ContextSignal, SignalSeverity, SignalType};

/// Auth-related patterns to detect in routes.
const AUTH_PATTERNS: &[&str] = &[
    "@login_required",
    "@require_auth",
    "@authenticated",
    "@permission_required",
    "@auth_required",
    "Depends(get_current_user",
    "authorize(",
    "isAuthenticated",
    "requireAuth",
    "authMiddleware",
    "@requires_auth",
    "protected_resource",
    "#[authorize",
];

/// Route handler patterns.
const ROUTE_PATTERNS: &[&str] = &[
    "@app.route(",
    "@router.",
    "@app.get(",
    "@app.post(",
    "@app.put(",
    "@app.delete(",
    "@app.patch(",
    "app.get(",
    "app.post(",
    "app.put(",
    "app.delete(",
    "router.get(",
    "router.post(",
    "#[get(",
    "#[post(",
    "#[put(",
    "#[delete(",
];

/// Detect auth coverage gaps.
pub fn detect(file_contents: &[(String, String)]) -> Vec<ContextSignal> {
    let mut unprotected_routes = Vec::new();
    let mut service_role_files = Vec::new();

    for (path, content) in file_contents {
        let lines: Vec<&str> = content.lines().collect();

        // Check for route handlers
        let has_routes = lines
            .iter()
            .any(|l| ROUTE_PATTERNS.iter().any(|p| l.contains(p)));

        if has_routes {
            let has_auth = lines
                .iter()
                .any(|l| AUTH_PATTERNS.iter().any(|p| l.contains(p)));

            if !has_auth {
                unprotected_routes.push(path.clone());
            }
        }

        // Check for service role in client code (Supabase pattern)
        let has_service_role = lines.iter().any(|l| {
            l.contains("service_role") || l.contains("SERVICE_ROLE") || l.contains("serviceRole")
        });
        let is_server_file = path.contains("/server/")
            || path.contains("/api/")
            || path.contains("/backend/")
            || path.contains("server.");
        if has_service_role && !is_server_file {
            service_role_files.push(path.clone());
        }
    }

    let mut signals = Vec::new();

    if unprotected_routes.len() >= 2 {
        signals.push(ContextSignal {
            signal_type: SignalType::AuthCoverage,
            severity: SignalSeverity::High,
            message: format!(
                "{} route files have no auth decorators/middleware — review auth coverage",
                unprotected_routes.len(),
            ),
            files: unprotected_routes,
            detail: serde_json::json!({"pattern": "unprotected_routes"}),
        });
    }

    if !service_role_files.is_empty() {
        signals.push(ContextSignal {
            signal_type: SignalType::AuthCoverage,
            severity: SignalSeverity::High,
            message: format!(
                "{} non-server files use service_role key — potential privilege escalation",
                service_role_files.len(),
            ),
            files: service_role_files,
            detail: serde_json::json!({"pattern": "service_role_in_client"}),
        });
    }

    signals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unprotected_routes() {
        let files = vec![
            (
                "src/routes/users.py".into(),
                "@app.route('/users')\ndef list_users():\n    pass".into(),
            ),
            (
                "src/routes/items.py".into(),
                "@app.route('/items')\ndef list_items():\n    pass".into(),
            ),
        ];

        let signals = detect(&files);
        assert!(signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "unprotected_routes")));
    }

    #[test]
    fn protected_routes_not_flagged() {
        let files = vec![
            (
                "src/routes/users.py".into(),
                "@login_required\n@app.route('/users')\ndef list_users():\n    pass".into(),
            ),
            (
                "src/routes/items.py".into(),
                "@login_required\n@app.route('/items')\ndef list_items():\n    pass".into(),
            ),
        ];

        let signals = detect(&files);
        assert!(!signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "unprotected_routes")));
    }

    #[test]
    fn detects_service_role_in_client() {
        let files = vec![(
            "src/components/admin.tsx".into(),
            "const client = createClient(url, service_role_key)".into(),
        )];

        let signals = detect(&files);
        assert!(signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "service_role_in_client")));
    }

    #[test]
    fn service_role_in_server_ok() {
        let files = vec![(
            "src/server/auth.py".into(),
            "client = create_client(url, service_role_key)".into(),
        )];

        let signals = detect(&files);
        assert!(!signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "service_role_in_client")));
    }
}
