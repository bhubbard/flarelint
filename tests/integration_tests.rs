use flarelint::config::CloudflareConfig;
use flarelint::rules::{RuleCategory, run_linter_on_target};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_node_compat_strictly_unsupported() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("worker.ts");
    fs::write(
        &file,
        r#"
import { spawn } from 'child_process';
import cluster from 'node:cluster';

export default {
    async fetch(req: Request) {
        return new Response("OK");
    }
}
"#,
    )
    .unwrap();

    let mut config = CloudflareConfig::default();
    config.add_flag("nodejs_compat");

    let report = run_linter_on_target(dir.path(), RuleCategory::NodeCompat, Some(config)).unwrap();
    assert_eq!(report.error_count, 2);
    assert!(report.diagnostics.iter().any(
        |d| d.rule == "node-compat/strictly-unsupported" && d.message.contains("child_process")
    ));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule == "node-compat/strictly-unsupported" && d.message.contains("cluster"))
    );
}

#[test]
fn test_node_compat_missing_flag() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("index.js");
    fs::write(
        &file,
        r#"
import crypto from 'node:crypto';
const path = require('path');
"#,
    )
    .unwrap();

    let config = CloudflareConfig::default(); // no nodejs_compat flag
    let report = run_linter_on_target(dir.path(), RuleCategory::NodeCompat, Some(config)).unwrap();
    assert_eq!(report.error_count, 2);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.rule == "node-compat/missing-flag")
    );
}

#[test]
fn test_node_compat_prefer_node_protocol() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("index.ts");
    fs::write(
        &file,
        r#"
import { Buffer } from 'buffer';
import { EventEmitter } from 'events';
"#,
    )
    .unwrap();

    let mut config = CloudflareConfig::default();
    config.add_flag("nodejs_compat");

    let report = run_linter_on_target(dir.path(), RuleCategory::NodeCompat, Some(config)).unwrap();
    assert_eq!(report.error_count, 0);
    assert_eq!(report.warning_count, 2);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.rule == "node-compat/prefer-node-protocol")
    );
}

#[test]
fn test_astro_frontmatter_linting() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("Page.astro");
    fs::write(
        &file,
        r#"---
import { exec } from 'child_process';
const title = "Astro Edge";
---
<html>
  <body>
    <h1>{title}</h1>
  </body>
</html>
"#,
    )
    .unwrap();

    let mut config = CloudflareConfig::default();
    config.add_flag("nodejs_compat");

    let report = run_linter_on_target(dir.path(), RuleCategory::NodeCompat, Some(config)).unwrap();
    assert_eq!(report.error_count, 1);
    let diag = &report.diagnostics[0];
    assert_eq!(diag.rule, "node-compat/strictly-unsupported");
    assert_eq!(diag.location.unwrap().line, 2);
}

#[test]
fn test_waituntil_floating_promise() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("worker.js");
    fs::write(
        &file,
        r#"
export default {
    async fetch(request, env, ctx) {
        fetch('https://analytics.example.com/log');
        return new Response("Logged");
    }
}
"#,
    )
    .unwrap();

    let report = run_linter_on_target(dir.path(), RuleCategory::WaitUntil, None).unwrap();
    assert_eq!(report.error_count, 1);
    let diag = &report.diagnostics[0];
    assert_eq!(diag.rule, "waituntil/unawaited-async");
    assert!(diag.message.contains("fetch"));
}

#[test]
fn test_waituntil_clean_when_wrapped() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("worker.js");
    fs::write(
        &file,
        r#"
export default {
    async fetch(request, env, ctx) {
        ctx.waitUntil(fetch('https://analytics.example.com/log'));
        await fetch('https://api.example.com/data');
        return new Response("OK");
    }
}
"#,
    )
    .unwrap();

    let report = run_linter_on_target(dir.path(), RuleCategory::WaitUntil, None).unwrap();
    assert_eq!(report.error_count, 0);
    assert_eq!(report.warning_count, 0);
}

#[test]
fn test_do_storage_unawaited_and_concurrent() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("counter.ts");
    fs::write(
        &file,
        r#"
export class Counter {
    constructor(state: DurableObjectState) {
        this.state = state;
    }

    async increment() {
        this.state.storage.put("count", 1);
        Promise.all([
            this.state.storage.put("a", 1),
            this.state.storage.put("b", 2)
        ]);
        return 1;
    }
}
"#,
    )
    .unwrap();

    let report = run_linter_on_target(dir.path(), RuleCategory::DoStorage, None).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule == "do-storage/unawaited-storage-op")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule == "do-storage/concurrent-write-hazard")
    );
}

#[test]
fn test_routes_json_limits_and_globs() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("_routes.json");

    // Test invalid glob
    fs::write(
        &file,
        r#"{
    "version": 1,
    "include": ["/api/*/profile"],
    "exclude": []
}"#,
    )
    .unwrap();

    let report = run_linter_on_target(dir.path(), RuleCategory::Routes, None).unwrap();
    assert_eq!(report.error_count, 1);
    assert_eq!(report.diagnostics[0].rule, "routes/invalid-glob");

    // Test exceeding 100 limit
    let mut includes = Vec::new();
    for i in 0..105 {
        includes.push(format!("\"/page-{}\"", i));
    }
    let large_routes = format!(
        r#"{{
    "version": 1,
    "include": [{}],
    "exclude": []
}}"#,
        includes.join(",")
    );
    fs::write(&file, large_routes).unwrap();

    let report2 = run_linter_on_target(dir.path(), RuleCategory::Routes, None).unwrap();
    assert!(
        report2
            .diagnostics
            .iter()
            .any(|d| d.rule == "routes/exceeds-limit")
    );
}

#[test]
fn test_routes_json_clean() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("_routes.json");
    fs::write(
        &file,
        r#"{
    "version": 1,
    "include": ["/*"],
    "exclude": ["/static/*", "/favicon.ico"]
}"#,
    )
    .unwrap();

    let report = run_linter_on_target(dir.path(), RuleCategory::Routes, None).unwrap();
    assert_eq!(report.error_count, 0);
    assert_eq!(report.warning_count, 0);
}

#[test]
fn test_comprehensive_check() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("wrangler.jsonc"),
        r#"{
    "name": "my-app",
    "compatibility_date": "2024-09-23",
    "compatibility_flags": ["nodejs_compat"]
}"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("worker.ts"),
        r#"
import { Buffer } from 'node:buffer';

export default {
    async fetch(req: Request, env: any, ctx: ExecutionContext) {
        ctx.waitUntil(fetch('https://log.example.com'));
        return new Response(Buffer.from("Hello").toString());
    }
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("_routes.json"),
        r#"{
    "version": 1,
    "include": ["/*"],
    "exclude": ["/assets/*"]
}"#,
    )
    .unwrap();

    let report = run_linter_on_target(dir.path(), RuleCategory::All, None).unwrap();
    assert_eq!(report.error_count, 0);
    assert_eq!(report.warning_count, 0);
    assert_eq!(report.total_files_scanned, 2);
}
