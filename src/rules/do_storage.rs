use oxc_ast::ast::*;
use std::path::Path;

use crate::diagnostics::Diagnostic;
use crate::parser::{offset_to_location, AstUnit};

pub struct DoStorageLinter<'a> {
    pub file_path: &'a Path,
    pub full_source: &'a str,
    pub diagnostics: Vec<Diagnostic>,
    in_transaction_depth: usize,
}

impl<'a> DoStorageLinter<'a> {
    pub fn new(file_path: &'a Path, full_source: &'a str) -> Self {
        Self {
            file_path,
            full_source,
            diagnostics: Vec::new(),
            in_transaction_depth: 0,
        }
    }

    pub fn lint_ast(&mut self, ast: &AstUnit<'_>) {
        for stmt in &ast.program.body {
            self.visit_statement(stmt, ast);
        }
    }

    fn extract_snippet(&self, target_line: usize) -> Option<String> {
        let lines: Vec<&str> = self.full_source.lines().collect();
        if target_line == 0 || target_line > lines.len() {
            return None;
        }
        Some(lines[target_line - 1].trim().to_string())
    }

    fn is_storage_member_call<'b>(&self, call: &'b CallExpression<'_>) -> Option<(&'b str, String)> {
        if let Some(mem) = call.callee.as_member_expression()
            && let Some(prop_name) = mem.static_property_name()
            && matches!(
                prop_name,
                "put" | "get" | "delete" | "deleteAll" | "list" | "getAlarm" | "setAlarm" | "deleteAlarm" | "sync" | "transaction" | "sql"
            ) {
                let obj = mem.object();
                if self.is_storage_object(obj) {
                    return Some((prop_name, self.format_member_expr(mem)));
                }
            }
        None
    }

    fn format_member_expr(&self, mem: &MemberExpression<'_>) -> String {
        let prop = mem.static_property_name().unwrap_or("method");
        if let Some(sub_mem) = mem.object().as_member_expression() {
            let sub_prop = sub_mem.static_property_name().unwrap_or("storage");
            format!("{}.{}", sub_prop, prop)
        } else {
            match mem.object() {
                Expression::Identifier(ident) => {
                    format!("{}.{}", ident.name, prop)
                }
                Expression::ThisExpression(_) => {
                    format!("this.{}", prop)
                }
                _ => format!("storage.{}", prop),
            }
        }
    }

    fn is_storage_object(&self, expr: &Expression<'_>) -> bool {
        if let Some(mem) = expr.as_member_expression() {
            if let Some(prop) = mem.static_property_name()
                && prop == "storage" {
                    return true;
                }
            if let Expression::ThisExpression(_) = mem.object() {
                return mem.static_property_name() == Some("storage")
                    || mem.static_property_name() == Some("ctx")
                    || mem.static_property_name() == Some("state");
            }
            if let Some(inner) = mem.object().as_member_expression() {
                return inner.static_property_name() == Some("storage");
            }
            return false;
        }

        match expr {
            Expression::Identifier(ident) => {
                ident.name == "storage" || ident.name == "state" || ident.name == "ctx"
            }
            _ => false,
        }
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>, ast: &AstUnit<'_>) {
        match stmt {
            Statement::ExportNamedDeclaration(export_named) => {
                if let Some(decl) = &export_named.declaration {
                    self.visit_declaration(decl, ast);
                }
            }
            Statement::ExportDefaultDeclaration(export_default) => match &export_default.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(cls) => {
                    self.visit_class(cls, ast);
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    self.visit_function(func, ast);
                }
                decl => {
                    if let Some(expr) = decl.as_expression() {
                        self.visit_expression(expr, ast);
                    }
                }
            },
            Statement::ExpressionStatement(expr_stmt) => {
                if let Expression::CallExpression(call) = &expr_stmt.expression
                    && let Some((method, display_name)) = self.is_storage_member_call(call)
                {
                    let loc = offset_to_location(
                        self.full_source,
                        ast.byte_offset,
                        expr_stmt.span.start,
                        expr_stmt.span.end,
                    );

                    let mut diag = Diagnostic::error(
                        "do-storage/unawaited-storage-op",
                        format!(
                            "Durable Object storage operation '{display_name}()' must be awaited to guarantee persistence and avoid concurrency hazards."
                        ),
                        self.file_path,
                    )
                    .with_location(loc)
                    .with_suggestion(
                        format!("Add 'await' before '{display_name}(...)'."),
                        Some(format!("await {display_name}(...)")),
                    );
                    diag.code_snippet = self.extract_snippet(loc.line);
                    self.diagnostics.push(diag);

                    if method == "transaction" {
                        self.in_transaction_depth += 1;
                        for arg in &call.arguments {
                            if let Some(expr) = arg.as_expression() {
                                self.visit_expression(expr, ast);
                            }
                        }
                        self.in_transaction_depth -= 1;
                        return;
                    }
                }
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
                if let Some(h) = &try_stmt.handler {
                    for s in &h.body.body {
                        self.visit_statement(s, ast);
                    }
                }
                if let Some(f) = &try_stmt.finalizer {
                    for s in &f.body {
                        self.visit_statement(s, ast);
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_declaration(&mut self, decl: &Declaration<'_>, ast: &AstUnit<'_>) {
        match decl {
            Declaration::ClassDeclaration(cls) => self.visit_class(cls, ast),
            Declaration::FunctionDeclaration(func) => self.visit_function(func, ast),
            Declaration::VariableDeclaration(var_decl) => {
                for d in &var_decl.declarations {
                    if let Some(init) = &d.init {
                        self.visit_expression(init, ast);
                    }
                }
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
        for elem in &cls.body.body {
            if let ClassElement::MethodDefinition(m) = elem
                && let Some(body) = &m.value.body {
                    for s in &body.statements {
                        self.visit_statement(s, ast);
                    }
                }
        }
    }

    fn visit_expression(&mut self, expr: &Expression<'_>, ast: &AstUnit<'_>) {
        match expr {
            Expression::CallExpression(call) => {
                if self.is_promise_all_hazard(call) {
                    let loc = offset_to_location(
                        self.full_source,
                        ast.byte_offset,
                        call.span.start,
                        call.span.end,
                    );

                    let mut diag = Diagnostic::warning(
                        "do-storage/concurrent-write-hazard",
                        "Concurrent writes in Promise.all risk non-atomic state mutations and write collisions in Durable Objects.",
                        self.file_path,
                    )
                    .with_location(loc)
                    .with_suggestion(
                        "Use 'storage.transaction(async (txn) => { ... })' for atomic batch writes.",
                        None,
                    );
                    diag.code_snippet = self.extract_snippet(loc.line);
                    self.diagnostics.push(diag);
                }

                if let Some((method, display_name)) = self.is_storage_member_call(call) {
                    if method == "transaction" {
                        if self.in_transaction_depth > 0 {
                            let loc = offset_to_location(
                                self.full_source,
                                ast.byte_offset,
                                call.span.start,
                                call.span.end,
                            );
                            let mut diag = Diagnostic::error(
                                "do-storage/nested-transaction",
                                "Nested Durable Object transactions are not supported by the runtime.",
                                self.file_path,
                            )
                            .with_location(loc);
                            diag.code_snippet = self.extract_snippet(loc.line);
                            self.diagnostics.push(diag);
                        }

                        self.in_transaction_depth += 1;
                        for arg in &call.arguments {
                            if let Some(arg_expr) = arg.as_expression() {
                                self.visit_expression(arg_expr, ast);
                            }
                        }
                        self.in_transaction_depth -= 1;
                        return;
                    }

                    if self.in_transaction_depth > 0 && (display_name.starts_with("this.") || display_name.starts_with("state.")) {
                        let loc = offset_to_location(
                            self.full_source,
                            ast.byte_offset,
                            call.span.start,
                            call.span.end,
                        );
                        let mut diag = Diagnostic::warning(
                            "do-storage/transaction-escape",
                            format!(
                                "Calling '{display_name}()' inside a transaction bypasses the atomic transaction instance."
                            ),
                            self.file_path,
                        )
                        .with_location(loc)
                        .with_suggestion(
                            "Use the transaction handle (e.g. 'txn.put()') provided to the transaction callback.",
                            None,
                        );
                        diag.code_snippet = self.extract_snippet(loc.line);
                        self.diagnostics.push(diag);
                    }
                }

                for arg in &call.arguments {
                    if let Some(arg_expr) = arg.as_expression() {
                        self.visit_expression(arg_expr, ast);
                    }
                }
            }
            Expression::AwaitExpression(aw) => {
                self.visit_expression(&aw.argument, ast);
            }
            Expression::ArrowFunctionExpression(arrow) => {
                for s in &arrow.body.statements {
                    self.visit_statement(s, ast);
                }
            }
            Expression::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    for s in &body.statements {
                        self.visit_statement(s, ast);
                    }
                }
            }
            Expression::ArrayExpression(arr) => {
                for el in &arr.elements {
                    if let Some(e) = el.as_expression() {
                        self.visit_expression(e, ast);
                    }
                }
            }
            Expression::ObjectExpression(obj) => {
                for prop in &obj.properties {
                    if let ObjectPropertyKind::ObjectProperty(p) = prop {
                        self.visit_expression(&p.value, ast);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_promise_all_hazard(&self, call: &CallExpression<'_>) -> bool {
        if let Some(mem) = call.callee.as_member_expression()
            && let Expression::Identifier(ident) = mem.object()
            && ident.name == "Promise"
            && mem.static_property_name() == Some("all")
            && let Some(first_arg) = call.arguments.first()
            && let Some(Expression::ArrayExpression(arr)) = first_arg.as_expression()
        {
            let write_ops = arr
                .elements
                .iter()
                .filter_map(|el| el.as_expression())
                .filter(|e| {
                    if let Expression::CallExpression(c) = e
                        && let Some((method, _)) = self.is_storage_member_call(c)
                    {
                        return matches!(method, "put" | "delete" | "deleteAll");
                    }
                    false
                })
                .count();
            return write_ops > 1;
        }
        false
    }
}

