use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::diagnostics::{Diagnostic, SourceLocation};

const MAX_PAGES_ROUTES_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutesConfig {
    pub version: u32,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

pub struct RoutesLinter<'a> {
    pub file_path: &'a Path,
    pub full_source: &'a str,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'a> RoutesLinter<'a> {
    pub fn new(file_path: &'a Path, full_source: &'a str) -> Self {
        Self {
            file_path,
            full_source,
            diagnostics: Vec::new(),
        }
    }

    pub fn lint(&mut self) {
        let parsed: Result<RoutesConfig, _> = serde_json::from_str(self.full_source);
        let config = match parsed {
            Ok(c) => c,
            Err(e) => {
                let diag = Diagnostic::error(
                    "routes/invalid-json",
                    format!("Malformed _routes.json: {}", e),
                    self.file_path,
                )
                .with_location(SourceLocation::new(1, 1, 0, 0));
                self.diagnostics.push(diag);
                return;
            }
        };

        if config.version != 1 {
            let loc = self.find_line_offset("\"version\"");
            let diag = Diagnostic::error(
                "routes/invalid-version",
                format!("Invalid version {}. Cloudflare Pages requires version: 1", config.version),
                self.file_path,
            )
            .with_location(loc)
            .with_suggestion("Set \"version\": 1", Some("\"version\": 1".to_string()));
            self.diagnostics.push(diag);
        }

        let total_rules = config.include.len() + config.exclude.len();
        if total_rules > MAX_PAGES_ROUTES_LIMIT {
            let diag = Diagnostic::error(
                "routes/exceeds-limit",
                format!(
                    "Total route count ({}) exceeds Cloudflare Pages limit of {} rules (include: {}, exclude: {}).",
                    total_rules, MAX_PAGES_ROUTES_LIMIT, config.include.len(), config.exclude.len()
                ),
                self.file_path,
            )
            .with_location(SourceLocation::new(1, 1, 0, 0))
            .with_suggestion(
                format!("Reduce rules to <= {} by consolidating paths with wildcards (e.g. '/api/*').", MAX_PAGES_ROUTES_LIMIT),
                None,
            );
            self.diagnostics.push(diag);
        } else if total_rules >= 90 {
            let diag = Diagnostic::warning(
                "routes/approaching-limit",
                format!(
                    "Total route count ({}) is approaching the Cloudflare Pages limit of {} rules.",
                    total_rules, MAX_PAGES_ROUTES_LIMIT
                ),
                self.file_path,
            )
            .with_location(SourceLocation::new(1, 1, 0, 0));
            self.diagnostics.push(diag);
        }

        if config.include.is_empty() {
            let diag = Diagnostic::error(
                "routes/missing-include",
                "Cloudflare Pages _routes.json requires at least 1 'include' rule (e.g. '/*').",
                self.file_path,
            )
            .with_location(SourceLocation::new(1, 1, 0, 0))
            .with_suggestion("Add '\"include\": [\"/*\"]'", Some("\"include\": [\"/*\"]".to_string()));
            self.diagnostics.push(diag);
        }

        let mut seen_includes = HashSet::new();
        for rule in &config.include {
            self.validate_pattern(rule);
            if !seen_includes.insert(rule) {
                let loc = self.find_line_offset(rule);
                let diag = Diagnostic::warning(
                    "routes/duplicate-rule",
                    format!("Duplicate include rule '{}'", rule),
                    self.file_path,
                )
                .with_location(loc)
                .with_suggestion(format!("Remove duplicate rule '{}'", rule), None);
                self.diagnostics.push(diag);
            }
        }

        let mut seen_excludes = HashSet::new();
        for rule in &config.exclude {
            self.validate_pattern(rule);
            if !seen_excludes.insert(rule) {
                let loc = self.find_line_offset(rule);
                let diag = Diagnostic::warning(
                    "routes/duplicate-rule",
                    format!("Duplicate exclude rule '{}'", rule),
                    self.file_path,
                )
                .with_location(loc)
                .with_suggestion(format!("Remove duplicate rule '{}'", rule), None);
                self.diagnostics.push(diag);
            }
        }

        for (i, r1) in config.include.iter().enumerate() {
            for (j, r2) in config.include.iter().enumerate() {
                if i != j && self.pattern_shadows(r1, r2) {
                    let loc = self.find_line_offset(r2);
                    let diag = Diagnostic::warning(
                        "routes/shadowed-rule",
                        format!("Include rule '{}' is redundant because it is already matched by '{}'.", r2, r1),
                        self.file_path,
                    )
                    .with_location(loc)
                    .with_suggestion(format!("Remove shadowed rule '{}'", r2), None);
                    self.diagnostics.push(diag);
                }
            }
        }

        for exc in &config.exclude {
            let matched_by_any = config.include.iter().any(|inc| self.pattern_matches_or_overlaps(inc, exc));
            if !matched_by_any && !config.include.is_empty() {
                let loc = self.find_line_offset(exc);
                let diag = Diagnostic::warning(
                    "routes/unmatched-exclude",
                    format!("Exclude rule '{}' is unnecessary because no include rule matches its path prefix.", exc),
                    self.file_path,
                )
                .with_location(loc)
                .with_suggestion(format!("Remove unneeded exclude rule '{}' to save quota", exc), None);
                self.diagnostics.push(diag);
            }
        }
    }

    fn validate_pattern(&mut self, pattern: &str) {
        let loc = self.find_line_offset(pattern);

        if !pattern.starts_with('/') {
            let diag = Diagnostic::error(
                "routes/invalid-path",
                format!("Route pattern '{}' must start with '/'", pattern),
                self.file_path,
            )
            .with_location(loc)
            .with_suggestion(format!("Prepend '/' to '/{}'", pattern.trim_start_matches('/')), Some(format!("/{}", pattern)));
            self.diagnostics.push(diag);
            return;
        }

        if let Some(star_pos) = pattern.find('*')
            && star_pos != pattern.len() - 1 {
                let diag = Diagnostic::error(
                    "routes/invalid-glob",
                    format!(
                        "Cloudflare Pages wildcards ('*') can only appear at the end of a route pattern (e.g. '/api/*'). In '{}', wildcard is at position {}.",
                        pattern, star_pos
                    ),
                    self.file_path,
                )
                .with_location(loc)
                .with_suggestion(
                    "Move wildcard to the end of the prefix or use exact matching.",
                    None,
                );
                self.diagnostics.push(diag);
            }

        if pattern.contains("//") {
            let diag = Diagnostic::warning(
                "routes/double-slash",
                format!("Route pattern '{}' contains consecutive slashes '//'", pattern),
                self.file_path,
            )
            .with_location(loc);
            self.diagnostics.push(diag);
        }
    }

    fn pattern_shadows(&self, broader: &str, narrower: &str) -> bool {
        if broader == "/*" && narrower != "/*" {
            return true;
        }
        if let Some(prefix) = broader.strip_suffix("/*")
            && narrower.starts_with(prefix) && narrower != broader {
                return true;
        }
        false
    }

    fn pattern_matches_or_overlaps(&self, include: &str, exclude: &str) -> bool {
        if include == "/*" {
            return true;
        }
        if let Some(prefix) = include.strip_suffix("/*")
            && exclude.starts_with(prefix) {
                return true;
        }
        if include == exclude {
            return true;
        }
        false
    }

    fn find_line_offset(&self, query: &str) -> SourceLocation {
        let mut offset = 0;

        for (line_num, line) in (1..).zip(self.full_source.lines()) {
            if let Some(col) = line.find(query) {
                return SourceLocation::new(line_num, col + 1, offset + col, query.len());
            }
            offset += line.len() + 1;
        }

        SourceLocation::new(1, 1, 0, query.len())
    }
}

