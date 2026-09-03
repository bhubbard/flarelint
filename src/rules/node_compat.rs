use oxc_ast::ast::*;
use std::path::Path;

use crate::config::CloudflareConfig;
use crate::diagnostics::Diagnostic;
use crate::parser::{AstUnit, offset_to_location};

pub const STRICTLY_UNSUPPORTED_MODULES: &[&str] = &[
    "child_process",
    "cluster",
    "dgram",
    "v8",
    "vm",
    "worker_threads",
];

pub const ALL_NODE_BUILTINS: &[&str] = &[
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

pub fn normalize_module_specifier(specifier: &str) -> &str {
    specifier.strip_prefix("node:").unwrap_or(specifier)
}

pub fn is_node_builtin(specifier: &str) -> bool {
    let clean = normalize_module_specifier(specifier);
    ALL_NODE_BUILTINS.contains(&clean)
}

pub fn is_strictly_unsupported(specifier: &str) -> bool {
    let clean = normalize_module_specifier(specifier);
    STRICTLY_UNSUPPORTED_MODULES.contains(&clean)
}

pub struct NodeCompatLinter<'a> {
    pub file_path: &'a Path,
    pub full_source: &'a str,
    pub config: &'a CloudflareConfig,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'a> NodeCompatLinter<'a> {
    pub fn new(file_path: &'a Path, full_source: &'a str, config: &'a CloudflareConfig) -> Self {
        Self {
            file_path,
            full_source,
            config,
            diagnostics: Vec::new(),
        }
    }

    pub fn lint_ast(&mut self, ast: &AstUnit<'_>) {
        for stmt in &ast.program.body {
            self.visit_statement(stmt, ast);
        }
    }

    fn check_specifier(
        &mut self,
        specifier: &str,
        span_start: u32,
        span_end: u32,
        ast: &AstUnit<'_>,
    ) {
        let is_node_prefixed = specifier.starts_with("node:");
        let base_module = normalize_module_specifier(specifier);

        if !is_node_builtin(specifier) && !is_node_prefixed {
            return;
        }

        let loc = offset_to_location(self.full_source, ast.byte_offset, span_start, span_end);

        if is_strictly_unsupported(base_module) {
            let mut diag = Diagnostic::error(
                "node-compat/strictly-unsupported",
                format!(
                    "Node.js built-in module '{}' is strictly unsupported in Cloudflare Workers and Astro edge runtime.",
                    specifier
                ),
                self.file_path,
            )
            .with_location(loc)
            .with_suggestion(
                format!(
                    "Remove dependency on '{}'. Use Web standard APIs (WebCrypto, fetch, Streams) or Cloudflare bindings.",
                    base_module
                ),
                None,
            );
            diag.code_snippet = self.extract_snippet(loc.line);
            self.diagnostics.push(diag);
            return;
        }

        let has_nodejs_compat = self.config.has_nodejs_compat();

        if !has_nodejs_compat {
            let mut diag = Diagnostic::error(
                "node-compat/missing-flag",
                format!(
                    "Node.js built-in module '{}' requires 'nodejs_compat' compatibility flag in wrangler config.",
                    specifier
                ),
                self.file_path,
            )
            .with_location(loc)
            .with_suggestion(
                "Add 'nodejs_compat' to 'compatibility_flags' in wrangler.jsonc or wrangler.toml",
                Some(format!("node:{}", base_module)),
            );
            diag.code_snippet = self.extract_snippet(loc.line);
            self.diagnostics.push(diag);
        } else if !is_node_prefixed && !self.config.has_nodejs_compat_v2() {
            let mut diag = Diagnostic::warning(
                "node-compat/prefer-node-protocol",
                format!(
                    "Import specifier '{}' should use explicit 'node:{}' prefix for Cloudflare Workers compatibility.",
                    specifier, base_module
                ),
                self.file_path,
            )
            .with_location(loc)
            .with_suggestion(
                format!("Change '{}' to 'node:{}'", specifier, base_module),
                Some(format!("node:{}", base_module)),
            );
            diag.code_snippet = self.extract_snippet(loc.line);
            self.diagnostics.push(diag);
        }
    }

    fn extract_snippet(&self, target_line: usize) -> Option<String> {
        let lines: Vec<&str> = self.full_source.lines().collect();
        if target_line == 0 || target_line > lines.len() {
            return None;
        }
        Some(lines[target_line - 1].trim().to_string())
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>, ast: &AstUnit<'_>) {
        match stmt {
            Statement::ImportDeclaration(import_decl) => {
                let specifier = import_decl.source.value.as_str();
                self.check_specifier(
                    specifier,
                    import_decl.source.span.start,
                    import_decl.source.span.end,
                    ast,
                );
            }
            Statement::ExportAllDeclaration(export_decl) => {
                let specifier = export_decl.source.value.as_str();
                self.check_specifier(
                    specifier,
                    export_decl.source.span.start,
                    export_decl.source.span.end,
                    ast,
                );
            }
            Statement::ExportNamedDeclaration(export_decl) => {
                if let Some(source) = &export_decl.source {
                    let specifier = source.value.as_str();
                    self.check_specifier(specifier, source.span.start, source.span.end, ast);
                }
                if let Some(decl) = &export_decl.declaration {
                    self.visit_declaration(decl, ast);
                }
            }
            Statement::ExportDefaultDeclaration(export_decl) => match &export_decl.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    self.visit_function(func, ast);
                }
                ExportDefaultDeclarationKind::ClassDeclaration(cls) => {
                    self.visit_class(cls, ast);
                }
                decl => {
                    if let Some(expr) = decl.as_expression() {
                        self.visit_expression(expr, ast);
                    }
                }
            },
            Statement::ExpressionStatement(expr_stmt) => {
                self.visit_expression(&expr_stmt.expression, ast);
            }
            Statement::BlockStatement(block) => {
                for s in &block.body {
                    self.visit_statement(s, ast);
                }
            }
            Statement::IfStatement(if_stmt) => {
                self.visit_expression(&if_stmt.test, ast);
                self.visit_statement(&if_stmt.consequent, ast);
                if let Some(alt) = &if_stmt.alternate {
                    self.visit_statement(alt, ast);
                }
            }
            Statement::VariableDeclaration(var_decl) => {
                for decl in &var_decl.declarations {
                    if let Some(init) = &decl.init {
                        self.visit_expression(init, ast);
                    }
                }
            }
            Statement::FunctionDeclaration(func) => {
                self.visit_function(func, ast);
            }
            Statement::ClassDeclaration(cls) => {
                self.visit_class(cls, ast);
            }
            Statement::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument {
                    self.visit_expression(arg, ast);
                }
            }
            Statement::TryStatement(try_stmt) => {
                for s in &try_stmt.block.body {
                    self.visit_statement(s, ast);
                }
                if let Some(handler) = &try_stmt.handler {
                    for s in &handler.body.body {
                        self.visit_statement(s, ast);
                    }
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    for s in &finalizer.body {
                        self.visit_statement(s, ast);
                    }
                }
            }
            Statement::ForStatement(for_stmt) => {
                if let Some(init) = &for_stmt.init {
                    match init {
                        ForStatementInit::VariableDeclaration(v) => {
                            for decl in &v.declarations {
                                if let Some(e) = &decl.init {
                                    self.visit_expression(e, ast);
                                }
                            }
                        }
                        other_init => {
                            if let Some(e) = other_init.as_expression() {
                                self.visit_expression(e, ast);
                            }
                        }
                    }
                }
                if let Some(test) = &for_stmt.test {
                    self.visit_expression(test, ast);
                }
                if let Some(update) = &for_stmt.update {
                    self.visit_expression(update, ast);
                }
                self.visit_statement(&for_stmt.body, ast);
            }
            Statement::ForInStatement(for_in) => {
                self.visit_expression(&for_in.right, ast);
                self.visit_statement(&for_in.body, ast);
            }
            Statement::ForOfStatement(for_of) => {
                self.visit_expression(&for_of.right, ast);
                self.visit_statement(&for_of.body, ast);
            }
            Statement::WhileStatement(while_stmt) => {
                self.visit_expression(&while_stmt.test, ast);
                self.visit_statement(&while_stmt.body, ast);
            }
            Statement::DoWhileStatement(dowhile) => {
                self.visit_statement(&dowhile.body, ast);
                self.visit_expression(&dowhile.test, ast);
            }
            Statement::SwitchStatement(switch_stmt) => {
                self.visit_expression(&switch_stmt.discriminant, ast);
                for case in &switch_stmt.cases {
                    if let Some(test) = &case.test {
                        self.visit_expression(test, ast);
                    }
                    for s in &case.consequent {
                        self.visit_statement(s, ast);
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_declaration(&mut self, decl: &Declaration<'_>, ast: &AstUnit<'_>) {
        match decl {
            Declaration::VariableDeclaration(var_decl) => {
                for d in &var_decl.declarations {
                    if let Some(init) = &d.init {
                        self.visit_expression(init, ast);
                    }
                }
            }
            Declaration::FunctionDeclaration(func) => {
                self.visit_function(func, ast);
            }
            Declaration::ClassDeclaration(cls) => {
                self.visit_class(cls, ast);
            }
            _ => {}
        }
    }

    fn visit_function(&mut self, func: &Function<'_>, ast: &AstUnit<'_>) {
        if let Some(body) = &func.body {
            for s in &body.statements {
                self.visit_statement(s, ast);
            }
        }
    }

    fn visit_class(&mut self, cls: &Class<'_>, ast: &AstUnit<'_>) {
        for element in &cls.body.body {
            match element {
                ClassElement::MethodDefinition(method) => {
                    if let Some(body) = &method.value.body {
                        for s in &body.statements {
                            self.visit_statement(s, ast);
                        }
                    }
                }
                ClassElement::PropertyDefinition(prop) => {
                    if let Some(value) = &prop.value {
                        self.visit_expression(value, ast);
                    }
                }
                ClassElement::AccessorProperty(acc) => {
                    if let Some(value) = &acc.value {
                        self.visit_expression(value, ast);
                    }
                }
                ClassElement::StaticBlock(block) => {
                    for s in &block.body {
                        self.visit_statement(s, ast);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_expression(&mut self, expr: &Expression<'_>, ast: &AstUnit<'_>) {
        if let Some(mem) = expr.as_member_expression() {
            self.visit_expression(mem.object(), ast);
            return;
        }

        match expr {
            Expression::CallExpression(call) => {
                if let Expression::Identifier(ident) = &call.callee
                    && ident.name == "require"
                    && !call.arguments.is_empty()
                    && let Some(first_arg) = call.arguments.first()
                    && let Some(Expression::StringLiteral(lit)) = first_arg.as_expression()
                {
                    self.check_specifier(lit.value.as_str(), lit.span.start, lit.span.end, ast);
                }
                self.visit_expression(&call.callee, ast);
                for arg in &call.arguments {
                    if let Some(arg_expr) = arg.as_expression() {
                        self.visit_expression(arg_expr, ast);
                    }
                }
            }
            Expression::ImportExpression(imp) => {
                if let Expression::StringLiteral(lit) = &imp.source {
                    self.check_specifier(lit.value.as_str(), lit.span.start, lit.span.end, ast);
                } else {
                    self.visit_expression(&imp.source, ast);
                }
            }
            Expression::ArrayExpression(arr) => {
                for elem in &arr.elements {
                    if let Some(elem_expr) = elem.as_expression() {
                        self.visit_expression(elem_expr, ast);
                    }
                }
            }
            Expression::ObjectExpression(obj) => {
                for prop in &obj.properties {
                    match prop {
                        ObjectPropertyKind::ObjectProperty(p) => {
                            self.visit_expression(&p.value, ast);
                        }
                        ObjectPropertyKind::SpreadProperty(p) => {
                            self.visit_expression(&p.argument, ast);
                        }
                    }
                }
            }
            Expression::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    if let Some(Statement::ExpressionStatement(e)) = arrow.body.statements.first() {
                        self.visit_expression(&e.expression, ast);
                    }
                } else {
                    for s in &arrow.body.statements {
                        self.visit_statement(s, ast);
                    }
                }
            }
            Expression::FunctionExpression(func) => {
                self.visit_function(func, ast);
            }
            Expression::AwaitExpression(aw) => {
                self.visit_expression(&aw.argument, ast);
            }
            Expression::BinaryExpression(bin) => {
                self.visit_expression(&bin.left, ast);
                self.visit_expression(&bin.right, ast);
            }
            Expression::LogicalExpression(log) => {
                self.visit_expression(&log.left, ast);
                self.visit_expression(&log.right, ast);
            }
            Expression::UnaryExpression(un) => {
                self.visit_expression(&un.argument, ast);
            }
            Expression::AssignmentExpression(assign) => {
                self.visit_expression(&assign.right, ast);
            }
            Expression::SequenceExpression(seq) => {
                for e in &seq.expressions {
                    self.visit_expression(e, ast);
                }
            }
            Expression::ParenthesizedExpression(paren) => {
                self.visit_expression(&paren.expression, ast);
            }
            Expression::ConditionalExpression(cond) => {
                self.visit_expression(&cond.test, ast);
                self.visit_expression(&cond.consequent, ast);
                self.visit_expression(&cond.alternate, ast);
            }
            Expression::NewExpression(new_expr) => {
                self.visit_expression(&new_expr.callee, ast);
                for arg in &new_expr.arguments {
                    if let Some(arg_expr) = arg.as_expression() {
                        self.visit_expression(arg_expr, ast);
                    }
                }
            }
            _ => {}
        }
    }
}
