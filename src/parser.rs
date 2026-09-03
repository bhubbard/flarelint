use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::diagnostics::SourceLocation;

#[derive(Debug, Clone)]
pub struct SourceScript<'a> {
    pub content: &'a str,
    pub source_type: SourceType,
    pub byte_offset: usize,
    pub is_astro_frontmatter: bool,
}

#[derive(Debug)]
pub struct ParsedFile<'a> {
    pub path: PathBuf,
    pub full_source: String,
    pub scripts: Vec<SourceScript<'a>>,
}

pub struct AstUnit<'a> {
    pub program: Program<'a>,
    pub byte_offset: usize,
    pub script_source: &'a str,
}

pub fn is_supported_source_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "js" | "mjs" | "cjs" | "ts" | "mts" | "cts" | "jsx" | "tsx" | "astro"
    )
}

pub fn is_routes_json_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name == "_routes.json")
}

pub fn discover_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !should_ignore_entry(e.path()))
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && (is_supported_source_file(path) || is_routes_json_file(path)) {
            files.push(path.to_path_buf());
        }
    }
    files.sort();
    files
}

pub fn should_ignore_entry(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        matches!(
            name,
            "node_modules"
                | "dist"
                | ".wrangler"
                | ".astro"
                | "build"
                | "target"
                | "coverage"
                | ".git"
                | ".turbo"
                | ".next"
        )
    } else {
        false
    }
}

pub fn extract_scripts<'a>(path: &Path, content: &'a str) -> Vec<SourceScript<'a>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "astro" {
        extract_astro_scripts(content)
    } else {
        let source_type = SourceType::from_path(path).unwrap_or_else(|_| {
            SourceType::default()
                .with_module(true)
                .with_typescript(true)
                .with_jsx(true)
        });
        vec![SourceScript {
            content,
            source_type,
            byte_offset: 0,
            is_astro_frontmatter: false,
        }]
    }
}

fn extract_astro_scripts(content: &str) -> Vec<SourceScript<'_>> {
    let mut scripts = Vec::new();
    let trimmed = content.trim_start();
    let leading_whitespace_len = content.len() - trimmed.len();

    if let Some(after_first) = trimmed.strip_prefix("---")
        && let Some(end_idx) = after_first.find("---")
    {
        let frontmatter = &after_first[..end_idx];
        let byte_offset = leading_whitespace_len + 3;
        scripts.push(SourceScript {
            content: frontmatter,
            source_type: SourceType::default()
                .with_module(true)
                .with_typescript(true)
                .with_jsx(true),
            byte_offset,
            is_astro_frontmatter: true,
        });
    }

    let mut search_pos = 0;
    while let Some(start_tag) = content[search_pos..].find("<script") {
        let actual_start = search_pos + start_tag;
        if let Some(tag_end) = content[actual_start..].find('>') {
            let body_start = actual_start + tag_end + 1;
            if let Some(close_tag) = content[body_start..].find("</script>") {
                let script_body = &content[body_start..body_start + close_tag];
                scripts.push(SourceScript {
                    content: script_body,
                    source_type: SourceType::default()
                        .with_module(true)
                        .with_typescript(true)
                        .with_jsx(true),
                    byte_offset: body_start,
                    is_astro_frontmatter: false,
                });
                search_pos = body_start + close_tag + 9;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if scripts.is_empty() {
        scripts.push(SourceScript {
            content,
            source_type: SourceType::default()
                .with_module(true)
                .with_typescript(true)
                .with_jsx(true),
            byte_offset: 0,
            is_astro_frontmatter: false,
        });
    }

    scripts
}

pub fn parse_ast<'a>(
    allocator: &'a Allocator,
    script: &SourceScript<'a>,
) -> Result<AstUnit<'a>, String> {
    let ret = Parser::new(allocator, script.content, script.source_type).parse();

    if !ret.errors.is_empty() {
        let first_err = &ret.errors[0];
        return Err(format!("Parse error: {}", first_err));
    }

    Ok(AstUnit {
        program: ret.program,
        byte_offset: script.byte_offset,
        script_source: script.content,
    })
}

pub fn offset_to_location(
    full_source: &str,
    script_byte_offset: usize,
    span_start: u32,
    span_end: u32,
) -> SourceLocation {
    let absolute_start = script_byte_offset + span_start as usize;
    let absolute_end = script_byte_offset + span_end as usize;
    let length = absolute_end.saturating_sub(absolute_start);

    let mut line = 1;
    let mut col = 1;

    let target_offset = absolute_start.min(full_source.len());
    let sub = &full_source[..target_offset];

    for c in sub.chars() {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    SourceLocation::new(line, col, target_offset, length)
}

pub fn read_file_string(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read file {}: {}", path.display(), e))
}
