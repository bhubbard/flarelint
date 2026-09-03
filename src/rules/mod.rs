pub mod do_storage;
pub mod node_compat;
pub mod routes;
pub mod waituntil;

use oxc_allocator::Allocator;
use std::path::Path;
use std::time::Instant;

use crate::config::CloudflareConfig;
use crate::diagnostics::{Diagnostic, LintReport};
use crate::parser::{
    discover_files, extract_scripts, is_routes_json_file, is_supported_source_file, parse_ast,
    read_file_string,
};
use do_storage::DoStorageLinter;
use node_compat::NodeCompatLinter;
use routes::RoutesLinter;
use waituntil::WaitUntilLinter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCategory {
    All,
    NodeCompat,
    WaitUntil,
    DoStorage,
    Routes,
}

pub fn run_linter_on_target(
    target_path: &Path,
    category: RuleCategory,
    override_config: Option<CloudflareConfig>,
) -> Result<LintReport, String> {
    let start_time = Instant::now();
    let mut report = LintReport::new();

    let config = if let Some(cfg) = override_config {
        cfg
    } else {
        CloudflareConfig::find_and_load(target_path).unwrap_or_default()
    };

    let files = discover_files(target_path);

    for file_path in files {
        if is_routes_json_file(&file_path) {
            if category == RuleCategory::All || category == RuleCategory::Routes {
                report.total_files_scanned += 1;
                if let Ok(content) = read_file_string(&file_path) {
                    let mut linter = RoutesLinter::new(&file_path, &content);
                    linter.lint();
                    for diag in linter.diagnostics {
                        report.add_diagnostic(diag);
                    }
                }
            }
            continue;
        }

        if !is_supported_source_file(&file_path) {
            continue;
        }

        if category == RuleCategory::Routes {
            continue;
        }

        report.total_files_scanned += 1;
        let content = match read_file_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                report.add_diagnostic(Diagnostic::error("io-error", e, &file_path));
                continue;
            }
        };

        let scripts = extract_scripts(&file_path, &content);
        for script in scripts {
            let allocator = Allocator::default();
            match parse_ast(&allocator, &script) {
                Ok(ast) => {
                    if category == RuleCategory::All || category == RuleCategory::NodeCompat {
                        let mut node_linter = NodeCompatLinter::new(&file_path, &content, &config);
                        node_linter.lint_ast(&ast);
                        for diag in node_linter.diagnostics {
                            report.add_diagnostic(diag);
                        }
                    }

                    if category == RuleCategory::All || category == RuleCategory::WaitUntil {
                        let mut wait_linter = WaitUntilLinter::new(&file_path, &content);
                        wait_linter.lint_ast(&ast);
                        for diag in wait_linter.diagnostics {
                            report.add_diagnostic(diag);
                        }
                    }

                    if category == RuleCategory::All || category == RuleCategory::DoStorage {
                        let mut do_linter = DoStorageLinter::new(&file_path, &content);
                        do_linter.lint_ast(&ast);
                        for diag in do_linter.diagnostics {
                            report.add_diagnostic(diag);
                        }
                    }
                }
                Err(err) => {
                    report.add_diagnostic(Diagnostic::error(
                        "syntax-error",
                        format!("Failed to parse source AST: {}", err),
                        &file_path,
                    ));
                }
            }
        }
    }

    report.elapsed_ns = start_time.elapsed().as_nanos();
    Ok(report)
}
