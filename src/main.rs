use clap::{Command, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use flarelint::config::CloudflareConfig;
use flarelint::formatter::{format_report_human, format_report_json};
use flarelint::rules::{run_linter_on_target, RuleCategory};

#[derive(Parser, Debug)]
#[command(
    name = "flarelint",
    author = "Brandon Hubbard",
    version,
    about = "Unified, nanosecond-fast Rust AST static analysis and edge compatibility linter for Cloudflare Workers & Astro applications",
    long_about = "flarelint consolidates AST rules for Cloudflare Workers, Pages, Durable Objects, and Astro edge applications, validating Node.js compatibility, floating promises, transaction safety, and _routes.json limits."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to lint directly (defaults to check command)
    #[arg(global = true)]
    path: Option<PathBuf>,

    /// Output report in JSON format
    #[arg(short = 'j', long = "json", global = true)]
    json: bool,

    /// Treat warnings as errors
    #[arg(long = "strict", global = true)]
    strict: bool,

    /// Show verbose diagnostics with inline code snippets and fix suggestions
    #[arg(short = 'v', long = "verbose", global = true)]
    verbose: bool,

    /// Additional Cloudflare compatibility flags (e.g. nodejs_compat, nodejs_compat_v2)
    #[arg(short = 'c', long = "compatibility-flag", global = true)]
    compatibility_flag: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Scans JS/TS/Astro AST for unsupported Node.js runtime built-in modules
    #[command(name = "node-compat")]
    NodeCompat {
        /// Target file or directory to scan (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Detects un-awaited async promises in request handlers not passed to ctx.waitUntil()
    #[command(name = "waituntil")]
    WaitUntil {
        /// Target file or directory to scan (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Lints Durable Object storage transaction patterns, concurrent write hazards, and un-awaited storage operations
    #[command(name = "do-storage")]
    DoStorage {
        /// Target file or directory to scan (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Lints _routes.json files for rule overlap, invalid globs, and Cloudflare Pages 100-rule limit
    #[command(name = "routes")]
    Routes {
        /// Target file or directory to scan (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Comprehensive end-to-end linting running all rules
    #[command(name = "check")]
    Check {
        /// Target file or directory to scan (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Generates shell completion scripts (zsh, bash, fish, powershell, elvish)
    #[command(name = "completions")]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: ShellChoice,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ShellChoice {
    Zsh,
    Bash,
    Fish,
    Powershell,
    Elvish,
}

impl From<ShellChoice> for Shell {
    fn from(choice: ShellChoice) -> Self {
        match choice {
            ShellChoice::Zsh => Shell::Zsh,
            ShellChoice::Bash => Shell::Bash,
            ShellChoice::Fish => Shell::Fish,
            ShellChoice::Powershell => Shell::PowerShell,
            ShellChoice::Elvish => Shell::Elvish,
        }
    }
}

fn print_completions<G: clap_complete::Generator>(generator: G, cmd: &mut Command) {
    generate(generator, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let mut custom_config = CloudflareConfig::default();
    for flag in &cli.compatibility_flag {
        custom_config.add_flag(flag);
    }
    let override_config = if !custom_config.compatibility_flags.is_empty() {
        Some(custom_config)
    } else {
        None
    };

    let (category, target_path) = match &cli.command {
        Some(Commands::Completions { shell }) => {
            let mut cmd = Cli::command();
            print_completions(Shell::from(*shell), &mut cmd);
            return ExitCode::SUCCESS;
        }
        Some(Commands::NodeCompat { path }) => (RuleCategory::NodeCompat, path.clone()),
        Some(Commands::WaitUntil { path }) => (RuleCategory::WaitUntil, path.clone()),
        Some(Commands::DoStorage { path }) => (RuleCategory::DoStorage, path.clone()),
        Some(Commands::Routes { path }) => (RuleCategory::Routes, path.clone()),
        Some(Commands::Check { path }) => (RuleCategory::All, path.clone()),
        None => {
            let path = cli.path.clone().unwrap_or_else(|| PathBuf::from("."));
            (RuleCategory::All, path)
        }
    };

    let report = match run_linter_on_target(&target_path, category, override_config) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("flarelint error: {}", err);
            return ExitCode::FAILURE;
        }
    };

    if cli.json {
        match format_report_json(&report) {
            Ok(json_str) => println!("{}", json_str),
            Err(e) => eprintln!("Error formatting JSON: {}", e),
        }
    } else {
        let output = format_report_human(&report, cli.verbose);
        print!("{}", output);
    }

    if report.has_errors() || (cli.strict && report.warning_count > 0) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

