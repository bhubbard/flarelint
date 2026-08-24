use oxc_ast::ast::*;
use std::path::Path;

use crate::diagnostics::Diagnostic;
use crate::parser::{offset_to_location, AstUnit};

pub struct WaitUntilLinter<'a> {
    pub file_path: &'a Path,
    pub full_source: &'a str,
    pub diagnostics: Vec<Diagnostic>,
    in_handler_scope: bool,
    handler_ctx_name: Option<String>,
}

impl<'a> WaitUntilLinter<'a> {
    pub fn new(file_path: &'a Path, full_source: &'a str) -> Self {
        Self {
            file_path,
            full_source,
            diagnostics: Vec::new(),
            in_handler_scope: false,
            handler_ctx_name: None,
        }
    }

    pub fn lint_ast(&mut self, ast: &AstUnit<'_>) {
        for stmt in &ast.program.body {
            self.visit_top_level_statement(stmt, ast);
        }
    }

    fn extract_snippet(&self, target_line: usize) -> Option<String> {
        let lines: Vec<&str> = self.full_source.lines().collect();
        if target_line == 0 || target_line > lines.len() {
            return None;
        }
        Some(lines[target_line - 1].trim().to_string())
    }

    fn visit_top_level_statement(&mut self, stmt: &Statement<'_>, ast: &AstUnit<'_>) {
        match stmt {
            Statement::ExportDefaultDeclaration(export_default) => {
                match &export_default.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                        self.lint_function_handler(func, "default", ast);
                    }
                    decl => {
                        if let Some(Expression::ObjectExpression(obj)) = decl.as_expression() {
                            for prop in &obj.properties {
                                if let ObjectPropertyKind::ObjectProperty(p) = prop {
                                    let key_name = self.get_property_key_name(&p.key);
                                    if matches!(
                                        key_name.as_deref(),
                                        Some("fetch" | "scheduled" | "queue" | "email" | "tail" | "trace")
                                    ) {
                                        self.lint_handler_property(p, key_name.as_deref().unwrap(), ast);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(export_named) => {
                if let Some(Declaration::FunctionDeclaration(func)) = &export_named.declaration
                    && let Some(ident) = &func.id
                    && matches!(ident.name.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "ALL" | "onRequest" | "onRequestGet" | "onRequestPost")
                {
                    self.lint_function_handler(func, ident.name.as_str(), ast);
                }
            }
            Statement::ExpressionStatement(expr_stmt) => {
                if let Expression::CallExpression(call) = &expr_stmt.expression
                    && self.is_addeventlistener_fetch(call)
                {
                    self.lint_event_listener_call(call, ast);
                }
            }
            _ => {}
        }
    }

    fn get_property_key_name(&self, key: &PropertyKey<'_>) -> Option<String> {
        match key {
            PropertyKey::StaticIdentifier(ident) => Some(ident.name.to_string()),
            PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
            _ => None,
        }
    }

    fn is_addeventlistener_fetch(&self, call: &CallExpression<'_>) -> bool {
        let is_listener = if let Expression::Identifier(ident) = &call.callee {
            ident.name == "addEventListener"
        } else if let Some(mem) = call.callee.as_member_expression() {
            if let Expression::Identifier(ident) = mem.object() {
                (ident.name == "self" || ident.name == "globalThis")
                    && mem.static_property_name() == Some("addEventListener")
            } else {
                false
            }
        } else {
            false
        };

        if is_listener
            && !call.arguments.is_empty()
            && let Some(first_arg) = call.arguments.first()
            && let Some(lit) = first_arg.as_expression().and_then(|e| match e {
                Expression::StringLiteral(s) => Some(s),
                _ => None,
            })
        {
            return lit.value == "fetch" || lit.value == "scheduled" || lit.value == "queue";
        }

        false
    }

    fn lint_handler_property(&mut self, prop: &ObjectProperty<'_>, handler_name: &str, ast: &AstUnit<'_>) {
        match &prop.value {
            Expression::FunctionExpression(func) => {
                self.lint_function_handler(func, handler_name, ast);
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let ctx_name = if arrow.params.items.len() >= 3 {
                    arrow.params.items.get(2).and_then(|p| self.get_param_name(&p.pattern))
                } else if arrow.params.items.len() == 1 {
                    arrow.params.items.first().and_then(|p| self.get_param_name(&p.pattern))
                } else {
                    None
                };

                let prev_in_scope = self.in_handler_scope;
                let prev_ctx = self.handler_ctx_name.clone();
                self.in_handler_scope = true;
                self.handler_ctx_name = ctx_name;

                for s in &arrow.body.statements {
                    self.visit_handler_statement(s, ast);
                }

                self.in_handler_scope = prev_in_scope;
                self.handler_ctx_name = prev_ctx;
            }
            _ => {}
        }
    }

    fn get_param_name(&self, pattern: &BindingPattern<'_>) -> Option<String> {
        match &pattern.kind {
            BindingPatternKind::BindingIdentifier(ident) => Some(ident.name.to_string()),
            _ => None,
        }
    }

    fn lint_function_handler(&mut self, func: &Function<'_>, _handler_name: &str, ast: &AstUnit<'_>) {
        let ctx_name = if func.params.items.len() >= 3 {
            func.params.items.get(2).and_then(|p| self.get_param_name(&p.pattern))
        } else if func.params.items.len() == 1 {
            func.params.items.first().and_then(|p| self.get_param_name(&p.pattern))
        } else {
            None
        };

        let prev_in_scope = self.in_handler_scope;
        let prev_ctx = self.handler_ctx_name.clone();
        self.in_handler_scope = true;
        self.handler_ctx_name = ctx_name;

        if let Some(body) = &func.body {
            for s in &body.statements {
                self.visit_handler_statement(s, ast);
            }
        }

        self.in_handler_scope = prev_in_scope;
        self.handler_ctx_name = prev_ctx;
    }

    fn lint_event_listener_call(&mut self, call: &CallExpression<'_>, ast: &AstUnit<'_>) {
        if call.arguments.len() >= 2
            && let Some(second_arg) = call.arguments.get(1)
            && let Some(expr) = second_arg.as_expression()
        {
            match expr {
                Expression::FunctionExpression(func) => {
                    self.lint_function_handler(func, "eventListener", ast);
                }
                Expression::ArrowFunctionExpression(arrow) => {
                    let event_name = arrow.params.items.first().and_then(|p| self.get_param_name(&p.pattern));
                    let prev_in_scope = self.in_handler_scope;
                    let prev_ctx = self.handler_ctx_name.clone();
                    self.in_handler_scope = true;
                    self.handler_ctx_name = event_name;

                    for s in &arrow.body.statements {
                        self.visit_handler_statement(s, ast);
                    }

                    self.in_handler_scope = prev_in_scope;
                    self.handler_ctx_name = prev_ctx;
                }
                _ => {}
            }
        }
    }

    fn is_wait_until_call(&self, call: &CallExpression<'_>) -> bool {
        if let Some(mem) = call.callee.as_member_expression()
            && let Some(prop_name) = mem.static_property_name()
        {
            return prop_name == "waitUntil";
        }
        false
    }

    fn visit_handler_statement(&mut self, stmt: &Statement<'_>, ast: &AstUnit<'_>) {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                self.check_handler_expression(&expr_stmt.expression, expr_stmt.span.start, expr_stmt.span.end, ast);
            }
            Statement::BlockStatement(block) => {
                for s in &block.body {
                    self.visit_handler_statement(s, ast);
                }
            }
            Statement::IfStatement(if_stmt) => {
                self.visit_handler_statement(&if_stmt.consequent, ast);
                if let Some(alt) = &if_stmt.alternate {
                    self.visit_handler_statement(alt, ast);
                }
            }
            Statement::TryStatement(try_stmt) => {
                for s in &try_stmt.block.body {
                    self.visit_handler_statement(s, ast);
                }
                if let Some(h) = &try_stmt.handler {
                    for s in &h.body.body {
                        self.visit_handler_statement(s, ast);
                    }
                }
                if let Some(f) = &try_stmt.finalizer {
                    for s in &f.body {
                        self.visit_handler_statement(s, ast);
                    }
                }
            }
            Statement::ForStatement(for_stmt) => {
                self.visit_handler_statement(&for_stmt.body, ast);
            }
            Statement::ForInStatement(for_in) => {
                self.visit_handler_statement(&for_in.body, ast);
            }
            Statement::ForOfStatement(for_of) => {
                self.visit_handler_statement(&for_of.body, ast);
            }
            Statement::WhileStatement(while_stmt) => {
                self.visit_handler_statement(&while_stmt.body, ast);
            }
            _ => {}
        }
    }

    fn is_suspect_floating_promise(&self, call: &CallExpression<'_>) -> Option<String> {
        if self.is_wait_until_call(call) {
            return None;
        }

        match &call.callee {
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if name == "fetch" || name.starts_with("send") || name.starts_with("log") || name.starts_with("save") || name.starts_with("track") || name.ends_with("Async") {
                    return Some(name.to_string());
                }
                if name != "console" && !name.starts_with("assert") {
                    return Some(format!("{}()", name));
                }
            }
            _ => {
                if let Some(mem) = call.callee.as_member_expression()
                    && let Some(prop) = mem.static_property_name()
                    && matches!(prop, "put" | "get" | "delete" | "list" | "post" | "send" | "track" | "query" | "exec" | "run" | "fetch" | "write")
                {
                    return Some(format!(".{}()", prop));
                }
            }
        }

        None
    }

    fn check_handler_expression(&mut self, expr: &Expression<'_>, span_start: u32, span_end: u32, ast: &AstUnit<'_>) {
        if let Expression::CallExpression(call) = expr
            && let Some(call_desc) = self.is_suspect_floating_promise(call)
        {
            let loc = offset_to_location(
                self.full_source,
                ast.byte_offset,
                span_start,
                span_end,
            );

            let ctx_var = self.handler_ctx_name.as_deref().unwrap_or("ctx");
            let mut diag = Diagnostic::error(
                "waituntil/unawaited-async",
                format!(
                    "Un-awaited asynchronous operation '{}' in request handler will be terminated early when the response completes.",
                    call_desc
                ),
                self.file_path,
            )
            .with_location(loc)
            .with_suggestion(
                format!(
                    "Pass promise to '{}.waitUntil(...)', or use 'await'.",
                    ctx_var
                ),
                Some(format!("{}.waitUntil(...)", ctx_var)),
            );
            diag.code_snippet = self.extract_snippet(loc.line);
            self.diagnostics.push(diag);
        }
    }
}

