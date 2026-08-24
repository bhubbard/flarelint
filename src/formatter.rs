use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};

use crate::diagnostics::{LintReport, Severity};

pub fn format_report_human(report: &LintReport, verbose: bool) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "\n{}\n",
        "─── FLARELINT // EDGE COMPATIBILITY & AST AUDITOR ───"
            .bold()
            .white()
    ));

    if report.diagnostics.is_empty() {
        let duration_ms = report.elapsed_ns as f64 / 1_000_000.0;
        let per_file_us = if report.total_files_scanned > 0 {
            (report.elapsed_ns as f64 / report.total_files_scanned as f64) / 1_000.0
        } else {
            0.0
        };

        out.push_str(&format!(
            "{} Clean! Scanned {} files in {:.2}ms ({:.1}µs/file). 0 errors, 0 warnings.\n\n",
            "✔".green().bold(),
            report.total_files_scanned.to_string().bold().white(),
            duration_ms,
            per_file_us
        ));
        return out;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("SEVERITY")
                .add_attribute(Attribute::Bold)
                .fg(Color::White),
            Cell::new("RULE")
                .add_attribute(Attribute::Bold)
                .fg(Color::White),
            Cell::new("LOCATION")
                .add_attribute(Attribute::Bold)
                .fg(Color::White),
            Cell::new("MESSAGE")
                .add_attribute(Attribute::Bold)
                .fg(Color::White),
        ]);

    for diag in &report.diagnostics {
        let (sev_str, sev_color) = match diag.severity {
            Severity::Error => ("ERROR".to_string(), Color::Red),
            Severity::Warning => ("WARN".to_string(), Color::Yellow),
        };

        let loc_str = if let Some(loc) = &diag.location {
            format!("{}:{}:{}", diag.file_path.display(), loc.line, loc.column)
        } else {
            diag.file_path.display().to_string()
        };

        let mut msg_content = diag.message.clone();
        if verbose {
            if let Some(snippet) = &diag.code_snippet {
                msg_content.push_str(&format!("\n  › {}", snippet.dimmed()));
            }
            if let Some(suggestion) = &diag.suggestion {
                msg_content.push_str(&format!("\n  ↳ Fix: {}", suggestion.message.cyan()));
            }
        }

        table.add_row(vec![
            Cell::new(sev_str).fg(sev_color).add_attribute(Attribute::Bold),
            Cell::new(&diag.rule).fg(Color::Cyan),
            Cell::new(loc_str).fg(Color::DarkGrey),
            Cell::new(msg_content),
        ]);
    }

    out.push_str(&table.to_string());
    out.push('\n');

    for diag in &report.diagnostics {
        if !verbose
            && let Some(suggestion) = &diag.suggestion
        {
            let loc_str = if let Some(loc) = &diag.location {
                format!("{}:{}", diag.file_path.display(), loc.line)
            } else {
                diag.file_path.display().to_string()
            };
            out.push_str(&format!(
                "  {} [{}] {}\n",
                "↳".cyan().bold(),
                loc_str.dimmed(),
                suggestion.message
            ));
        }
    }

    let duration_ms = report.elapsed_ns as f64 / 1_000_000.0;
    let status_str = if report.has_errors() {
        "FAILED".red().bold()
    } else {
        "PASSED (WITH WARNINGS)".yellow().bold()
    };

    out.push_str(&format!(
        "\n{} // Scanned {} files in {:.2}ms // {} errors, {} warnings\n\n",
        status_str,
        report.total_files_scanned.to_string().bold(),
        duration_ms,
        report.error_count.to_string().bold().red(),
        report.warning_count.to_string().bold().yellow()
    ));

    out
}

pub fn format_report_json(report: &LintReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|e| format!("Failed to serialize JSON report: {}", e))
}
