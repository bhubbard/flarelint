use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudflareConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub compatibility_date: Option<String>,
    #[serde(default)]
    pub compatibility_flags: Vec<String>,
    #[serde(default)]
    pub pages_build_output_dir: Option<String>,
    #[serde(skip)]
    pub config_path: Option<PathBuf>,
}

impl CloudflareConfig {
    pub fn has_nodejs_compat(&self) -> bool {
        self.compatibility_flags
            .iter()
            .any(|f| f == "nodejs_compat" || f == "nodejs_compat_v2" || f == "nodejs_als")
    }

    pub fn has_nodejs_compat_v2(&self) -> bool {
        self.compatibility_flags
            .iter()
            .any(|f| f == "nodejs_compat_v2")
    }

    pub fn has_flag(&self, flag: &str) -> bool {
        self.compatibility_flags.iter().any(|f| f == flag)
    }

    pub fn add_flag(&mut self, flag: impl Into<String>) {
        let f = flag.into();
        if !self.compatibility_flags.contains(&f) {
            self.compatibility_flags.push(f);
        }
    }

    pub fn find_and_load(start_dir: &Path) -> Option<Self> {
        let mut current = if start_dir.is_file() {
            start_dir.parent()
        } else {
            Some(start_dir)
        };

        while let Some(dir) = current {
            let candidates = ["wrangler.jsonc", "wrangler.json", "wrangler.toml"];
            for candidate in candidates {
                let file = dir.join(candidate);
                if file.is_file()
                    && let Ok(config) = Self::load_file(&file)
                {
                    return Some(config);
                }
            }
            current = dir.parent();
        }

        None
    }

    pub fn load_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mut config: Self = match ext {
            "json" | "jsonc" => {
                let clean_json = strip_jsonc_comments(&content);
                serde_json::from_str(&clean_json)
                    .map_err(|e| format!("Failed to parse JSON config {}: {}", path.display(), e))?
            }
            "toml" => parse_simple_toml(&content)?,
            _ => {
                let clean_json = strip_jsonc_comments(&content);
                if let Ok(cfg) = serde_json::from_str(&clean_json) {
                    cfg
                } else {
                    parse_simple_toml(&content)?
                }
            }
        };

        config.config_path = Some(path.to_path_buf());
        Ok(config)
    }
}

fn strip_jsonc_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(c) = chars.next() {
        if in_string {
            output.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            output.push(c);
            continue;
        }

        if c == '/'
            && let Some(&next) = chars.peek()
        {
            if next == '/' {
                chars.next();
                for line_c in chars.by_ref() {
                    if line_c == '\n' {
                        output.push('\n');
                        break;
                    }
                }
                continue;
            } else if next == '*' {
                chars.next();
                let mut prev = ' ';
                for block_c in chars.by_ref() {
                    if prev == '*' && block_c == '/' {
                        break;
                    }
                    if block_c == '\n' {
                        output.push('\n');
                    }
                    prev = block_c;
                }
                continue;
            }
        }

        output.push(c);
    }

    output
}

fn parse_simple_toml(content: &str) -> Result<CloudflareConfig, String> {
    let mut config = CloudflareConfig::default();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();

            match key {
                "name" => {
                    config.name = Some(val.trim_matches('"').trim_matches('\'').to_string());
                }
                "main" => {
                    config.main = Some(val.trim_matches('"').trim_matches('\'').to_string());
                }
                "compatibility_date" => {
                    config.compatibility_date =
                        Some(val.trim_matches('"').trim_matches('\'').to_string());
                }
                "compatibility_flags" => {
                    let cleaned = val.trim_matches('[').trim_matches(']');
                    for flag in cleaned.split(',') {
                        let clean_flag = flag.trim().trim_matches('"').trim_matches('\'');
                        if !clean_flag.is_empty() {
                            config.compatibility_flags.push(clean_flag.to_string());
                        }
                    }
                }
                "pages_build_output_dir" => {
                    config.pages_build_output_dir =
                        Some(val.trim_matches('"').trim_matches('\'').to_string());
                }
                _ => {}
            }
        }
    }

    Ok(config)
}
