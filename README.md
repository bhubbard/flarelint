# flarelint

> **Unified, nanosecond-fast Rust AST static analysis and edge compatibility linter for Cloudflare Workers & Astro applications.**

`flarelint` consolidates `oxlint-plugin-cloudflare`, `oxc-astro-cf`, `oxc-cf-router`, `oxc-cf-assets`, `oxc-do-storage`, and `edge-compat-auditor` into a high-performance native binary written in Rust 2024 using the `oxc` AST engine.

---

## Features

- ⚡ **Nanosecond-Fast Execution**: Powered by `oxc_parser` and `oxc_allocator` for sub-millisecond parsing and analysis.
- 🌐 **Node.js Edge Compatibility Audit**: Scans JS/TS/Astro AST for unsupported Node.js runtime built-in modules (`fs`, `child_process`, `net`, `tls`, `dgram`, `cluster`, `v8`, `vm`, `worker_threads`).
- ⏳ **`ctx.waitUntil()` Promise Safety**: Catches floating un-awaited async operations inside request handlers before they get terminated by the runtime.
- 💾 **Durable Object Storage Validator**: Audits atomic transactions, prevents concurrent `Promise.all` write hazards, and enforces `await` on storage methods (`this.ctx.storage.put`).
- 🛣️ **Pages `_routes.json` Limiter**: Verifies the Cloudflare Pages 100-rule limit, detects invalid wildcard placements, catches duplicate/shadowed rules, and identifies wasted quota.
- 🚀 **First-Class Astro Support**: Parses Astro frontmatter scripts (`--- ... ---`) and `<script>` blocks with source location mapping.
- ⚙️ **Automatic Wrangler Configuration**: Automatically detects and parses `wrangler.jsonc`, `wrangler.json`, and `wrangler.toml` for `compatibility_flags`.

---

## Installation

### Cargo
```bash
cargo install --git https://github.com/bhubbard/flarelint.git --locked
```

### GitHub Actions
```yaml
- name: Run Flarelint
  uses: bhubbard/flarelint@main
  with:
    command: check
    path: .
    strict: true
```

---

## CLI Commands

### 1. `flarelint check [PATH]`
Runs all linting rules across JavaScript, TypeScript, JSX, TSX, Astro, and `_routes.json` files.
```bash
flarelint check .
flarelint check src/ --strict --json
```

### 2. `flarelint node-compat [PATH]`
Scans AST imports, `require()` calls, and dynamic `import()` specifiers against Cloudflare Workers runtime capabilities.
```bash
flarelint node-compat src/
```

- Flagged strictly unsupported modules: `child_process`, `cluster`, `dgram`, `v8`, `vm`, `worker_threads`.
- Automatically checks for `nodejs_compat` in your `wrangler.jsonc` or `wrangler.toml`.
- Suggests `node:` prefix specifiers where bare imports are detected.

### 3. `flarelint waituntil [PATH]`
Detects floating un-awaited promises spawned inside request and queue handlers.
```bash
flarelint waituntil src/
```
```typescript
// ❌ FAILS: Un-awaited async call terminated when response finishes
export default {
  async fetch(req, env, ctx) {
    sendTelemetry(req);
    return new Response("OK");
  }
}

// ✔️ PASSES: Handled via ctx.waitUntil
export default {
  async fetch(req, env, ctx) {
    ctx.waitUntil(sendTelemetry(req));
    return new Response("OK");
  }
}
```

### 4. `flarelint do-storage [PATH]`
Audits Durable Object storage operations, transactional safety, and concurrency hazards.
```bash
flarelint do-storage src/
```
```typescript
// ❌ FAILS: Un-awaited storage call
this.ctx.storage.put("key", value);

// ❌ FAILS: Concurrent write hazard in Promise.all
Promise.all([this.ctx.storage.put("a", 1), this.ctx.storage.put("b", 2)]);

// ✔️ PASSES: Transaction wrapper
await this.ctx.storage.transaction(async (txn) => {
  await txn.put("a", 1);
  await txn.put("b", 2);
});
```

### 5. `flarelint routes [PATH]`
Validates Cloudflare Pages `_routes.json` files for routing limits, invalid wildcards, and rule shadowing.
```bash
flarelint routes public/_routes.json
```

- Enforces Cloudflare Pages **100-rule limit** (`include + exclude <= 100`).
- Validates wildcard `*` positions (must be trailing or `/*`).
- Warns on shadowed rules (e.g. `/*` shadowing `/api/*`).
- Flags unneeded exclude rules that are not matched by any include prefix.

### 6. `flarelint completions <SHELL>`
Generates shell completion scripts for `zsh`, `bash`, `fish`, `powershell`, or `elvish`.
```bash
flarelint completions zsh > ~/.zsh/completion/_flarelint
```

---

## Global Options

| Option | Description |
|---|---|
| `-j, --json` | Output machine-readable JSON lint report |
| `--strict` | Treat warnings as errors (exits with code 1) |
| `-v, --verbose` | Show inline code snippets and fix recommendations |
| `-c, --compatibility-flag <FLAG>` | Pass additional Cloudflare compatibility flags (e.g. `nodejs_compat`) |
| `-h, --help` | Display help information |

---

## Rules Reference

| Rule ID | Severity | Description |
|---|---|---|
| `node-compat/strictly-unsupported` | ERROR | Module (`child_process`, `vm`, `cluster`, etc.) is strictly unsupported on edge |
| `node-compat/missing-flag` | ERROR | Node built-in used without `nodejs_compat` compatibility flag in wrangler config |
| `node-compat/prefer-node-protocol` | WARN | Import should use explicit `node:` prefix |
| `waituntil/unawaited-async` | ERROR | Asynchronous promise in request handler is un-awaited and not passed to `ctx.waitUntil` |
| `do-storage/unawaited-storage-op` | ERROR | Durable Object storage operation must be awaited |
| `do-storage/concurrent-write-hazard` | WARN | Concurrent writes in `Promise.all` risk non-atomic state mutations |
| `do-storage/nested-transaction` | ERROR | Nested transactions are unsupported by the Durable Object runtime |
| `do-storage/transaction-escape` | WARN | Calling instance storage inside a transaction callback bypasses transaction atomicity |
| `routes/exceeds-limit` | ERROR | Combined `include` and `exclude` rules exceed Cloudflare Pages limit of 100 rules |
| `routes/missing-include` | ERROR | `_routes.json` must have at least 1 include rule |
| `routes/invalid-glob` | ERROR | Asterisk wildcards can only appear at the end of route patterns |
| `routes/invalid-path` | ERROR | Route patterns must start with `/` |
| `routes/duplicate-rule` | WARN | Identical rule is defined multiple times |
| `routes/shadowed-rule` | WARN | Narrower rule is shadowed by a broader include prefix |
| `routes/unmatched-exclude` | WARN | Exclude rule is not matched by any include rule, wasting rule quota |

---

## License

MIT OR Apache-2.0 © [Brandon Hubbard](https://github.com/bhubbard)
