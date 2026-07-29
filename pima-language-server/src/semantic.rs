use std::collections::HashMap;

use pima::{
    source::Span,
    syntax::ast::{BindingKind, BlockId, Module, Name, NodeId, NodeKind, Pattern},
};

pub type SymbolId = usize;
type ScopeId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Binding,
    Function,
    Parameter,
    PatternCapture,
}

impl SymbolKind {
    pub fn description(self) -> &'static str {
        match self {
            Self::Binding => "binding",
            Self::Function => "function",
            Self::Parameter => "parameter",
            Self::PatternCapture => "pattern capture",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub declaration: Span,
    pub kind: SymbolKind,
    pub parameters: Vec<String>,
    pub mutable: bool,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub span: Span,
    pub symbol: SymbolId,
}

#[derive(Clone, Debug)]
pub struct SemanticIssue {
    pub span: Span,
    pub message: String,
    pub severity: IssueSeverity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticModel {
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    scopes: Vec<Scope>,
    issues: Vec<SemanticIssue>,
}

impl SemanticModel {
    pub fn build(module: &Module) -> Self {
        Builder::new(module).build()
    }

    pub fn symbol_at(&self, offset: usize) -> Option<SymbolId> {
        self.symbols
            .iter()
            .enumerate()
            .find(|(_, symbol)| contains(symbol.declaration, offset))
            .map(|(id, _)| id)
            .or_else(|| {
                self.references
                    .iter()
                    .find(|reference| contains(reference.span, offset))
                    .map(|reference| reference.symbol)
            })
    }

    pub fn occurrence_span(&self, symbol: SymbolId, offset: usize) -> Option<Span> {
        let declaration = self.symbols[symbol].declaration;
        if contains(declaration, offset) {
            return Some(declaration);
        }
        self.references
            .iter()
            .find(|reference| reference.symbol == symbol && contains(reference.span, offset))
            .map(|reference| reference.span)
    }

    pub fn reference_spans(&self, symbol: SymbolId, include_declaration: bool) -> Vec<Span> {
        let declarations = include_declaration
            .then(|| self.symbols[symbol].declaration)
            .into_iter();
        declarations
            .chain(
                self.references
                    .iter()
                    .filter(move |reference| reference.symbol == symbol)
                    .map(|reference| reference.span),
            )
            .collect()
    }

    pub fn visible_symbols_at(&self, offset: usize) -> Vec<SymbolId> {
        let mut scope = self
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| {
                scope
                    .range
                    .is_some_and(|range| contains_inclusive(range, offset))
            })
            .min_by_key(|(_, scope)| {
                let range = scope.range.expect("filtered to ranged scopes");
                range.end.saturating_sub(range.start)
            })
            .map_or(0, |(id, _)| id);
        let mut visible = HashMap::<&str, SymbolId>::new();
        loop {
            for (name, symbol) in &self.scopes[scope].definitions {
                if self.symbols[*symbol].declaration.start <= offset {
                    visible.entry(name).or_insert(*symbol);
                }
            }
            let Some(parent) = self.scopes[scope].parent else {
                break;
            };
            scope = parent;
        }
        let mut symbols = visible.into_values().collect::<Vec<_>>();
        symbols.sort_by(|left, right| self.symbols[*left].name.cmp(&self.symbols[*right].name));
        symbols
    }

    pub fn naming_issues(&self) -> Vec<SemanticIssue> {
        self.symbols
            .iter()
            .filter(|symbol| {
                matches!(
                    symbol.kind,
                    SymbolKind::Function | SymbolKind::Parameter | SymbolKind::PatternCapture
                ) && symbol
                    .name
                    .chars()
                    .any(|character| character.is_ascii_uppercase())
            })
            .map(|symbol| SemanticIssue {
                span: symbol.declaration,
                message: format!(
                    "{} `{}` should use snake_case",
                    symbol.kind.description(),
                    symbol.name
                ),
                severity: IssueSeverity::Warning,
            })
            .collect()
    }

    pub fn issues(&self) -> impl Iterator<Item = &SemanticIssue> {
        self.issues.iter()
    }
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

fn contains_inclusive(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

#[derive(Clone, Debug, Default)]
struct Scope {
    parent: Option<ScopeId>,
    range: Option<Span>,
    definitions: HashMap<String, SymbolId>,
}

struct Builder<'module> {
    module: &'module Module,
    scopes: Vec<Scope>,
    model: SemanticModel,
}

impl<'module> Builder<'module> {
    fn new(module: &'module Module) -> Self {
        let range = module
            .nodes
            .iter()
            .map(|node| node.span)
            .reduce(|left, right| {
                Span::new(
                    left.source,
                    left.start.min(right.start),
                    left.end.max(right.end),
                )
            });
        Self {
            module,
            scopes: vec![Scope {
                range,
                ..Scope::default()
            }],
            model: SemanticModel::default(),
        }
    }

    fn build(mut self) -> SemanticModel {
        self.visit_statements(&self.module.statements, 0);
        self.model.scopes = self.scopes;
        self.model
    }

    fn child_scope(&mut self, parent: ScopeId, range: Span) -> ScopeId {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            parent: Some(parent),
            range: Some(range),
            definitions: HashMap::new(),
        });
        id
    }

    fn define(&mut self, scope: ScopeId, name: &Name, kind: SymbolKind, mutable: bool) -> SymbolId {
        let id = self.model.symbols.len();
        self.model.symbols.push(Symbol {
            name: name.text.to_string(),
            declaration: name.span,
            kind,
            parameters: Vec::new(),
            mutable,
        });
        self.scopes[scope]
            .definitions
            .insert(name.text.to_string(), id);
        id
    }

    fn resolve(&self, mut scope: ScopeId, name: &str) -> Option<SymbolId> {
        loop {
            if let Some(symbol) = self.scopes[scope].definitions.get(name) {
                return Some(*symbol);
            }
            scope = self.scopes[scope].parent?;
        }
    }

    fn reference(&mut self, scope: ScopeId, name: &str, span: Span) {
        if let Some(symbol) = self.resolve(scope, name) {
            self.model.references.push(Reference { span, symbol });
        }
    }

    fn visit_statements(&mut self, statements: &[NodeId], scope: ScopeId) {
        for statement in statements {
            self.visit_node(*statement, scope);
        }
    }

    fn visit_block(&mut self, block: BlockId, scope: ScopeId) {
        let block = self.module.block(block);
        for requirement in &block.requirements {
            self.reference(scope, &requirement.text, requirement.span);
        }
        self.visit_statements(&block.statements, scope);
    }

    fn visit_node(&mut self, id: NodeId, scope: ScopeId) {
        let node = self.module.node(id);
        match &node.kind {
            NodeKind::Identifier(name) => self.reference(scope, name, node.span),
            NodeKind::List(elements) => {
                for element in elements {
                    self.visit_node(*element, scope);
                }
            }
            NodeKind::Block(block) => self.visit_block(*block, scope),
            NodeKind::Member { object, .. } => self.visit_node(*object, scope),
            NodeKind::Call {
                callee, arguments, ..
            } => {
                if let NodeKind::Identifier(name) = &self.module.node(*callee).kind
                    && let Some(symbol) = self.resolve(scope, name)
                    && self.model.symbols[symbol].kind == SymbolKind::Function
                    && arguments.len() > self.model.symbols[symbol].parameters.len()
                {
                    self.model.issues.push(SemanticIssue {
                        span: self.module.node(*callee).span,
                        message: format!(
                            "function `{}` accepts at most {} arguments, but {} were supplied",
                            name,
                            self.model.symbols[symbol].parameters.len(),
                            arguments.len()
                        ),
                        severity: IssueSeverity::Error,
                    });
                }
                self.visit_node(*callee, scope);
                for argument in arguments {
                    self.visit_node(*argument, scope);
                }
            }
            NodeKind::Binding {
                pattern,
                value,
                mutability,
                ..
            } => {
                self.visit_node(*value, scope);
                self.define_pattern(
                    scope,
                    pattern,
                    SymbolKind::Binding,
                    *mutability == BindingKind::Mutable,
                );
            }
            NodeKind::Assignment { pattern, value } => {
                self.visit_node(*value, scope);
                self.reference_pattern(scope, pattern);
            }
            NodeKind::Function {
                name,
                parameters,
                body,
                ..
            } => {
                let function = self.define(scope, name, SymbolKind::Function, false);
                self.model.symbols[function].parameters = parameters
                    .iter()
                    .map(|parameter| parameter.text.to_string())
                    .collect();
                let function_scope = self.child_scope(scope, self.module.block(*body).span);
                for parameter in parameters {
                    self.define(function_scope, parameter, SymbolKind::Parameter, false);
                }
                self.visit_block(*body, function_scope);
            }
            NodeKind::Conditional {
                condition,
                consequent,
                alternative,
            } => {
                self.visit_node(*condition, scope);
                self.visit_node(*consequent, scope);
                if let Some(alternative) = alternative {
                    self.visit_node(*alternative, scope);
                }
            }
            NodeKind::Loop {
                condition, body, ..
            } => {
                self.visit_node(*condition, scope);
                self.visit_block(*body, scope);
            }
            NodeKind::Return(value) | NodeKind::Break(value) => {
                if let Some(value) = value {
                    self.visit_node(*value, scope);
                }
            }
            NodeKind::Throw(value) | NodeKind::New(value) | NodeKind::Do(value) => {
                self.visit_node(*value, scope);
            }
            NodeKind::Attempt(block) => self.visit_block(*block, scope),
            NodeKind::Match { value, arms } => {
                self.visit_node(*value, scope);
                for arm in arms {
                    let arm_scope = self.child_scope(scope, self.module.block(arm.body).span);
                    self.define_pattern(arm_scope, &arm.pattern, SymbolKind::PatternCapture, false);
                    self.visit_block(arm.body, arm_scope);
                }
            }
            NodeKind::Unit
            | NodeKind::Boolean(_)
            | NodeKind::Integer(_)
            | NodeKind::Float(_)
            | NodeKind::String(_)
            | NodeKind::Symbol(_)
            | NodeKind::Placeholder
            | NodeKind::Continue
            | NodeKind::Import { .. }
            | NodeKind::StaticImport { .. } => {}
        }
    }

    fn define_pattern(
        &mut self,
        scope: ScopeId,
        pattern: &Pattern,
        kind: SymbolKind,
        mutable: bool,
    ) {
        match pattern {
            Pattern::Capture(name) => {
                self.define(scope, name, kind, mutable);
            }
            Pattern::List(elements) => {
                for element in elements {
                    self.define_pattern(scope, element, kind, mutable);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) => {}
        }
    }

    fn reference_pattern(&mut self, scope: ScopeId, pattern: &Pattern) {
        match pattern {
            Pattern::Capture(name) => {
                if let Some(symbol) = self.resolve(scope, &name.text) {
                    self.model.references.push(Reference {
                        span: name.span,
                        symbol,
                    });
                    if !self.model.symbols[symbol].mutable {
                        self.model.issues.push(SemanticIssue {
                            span: name.span,
                            message: format!("cannot assign to immutable binding `{}`", name.text),
                            severity: IssueSeverity::Error,
                        });
                    }
                }
            }
            Pattern::List(elements) => {
                for element in elements {
                    self.reference_pattern(scope, element);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use pima::{
        source::SourceMap,
        syntax::{lexer::lex, parser::parse},
    };

    use super::*;

    fn model(source: &str) -> SemanticModel {
        let mut sources = SourceMap::default();
        let source_id = sources.add("<test>", source);
        let tokens = lex(source_id, source).expect("lex");
        let module = parse(&tokens).expect("parse");
        SemanticModel::build(&module)
    }

    #[test]
    fn resolves_parameters_locals_and_recursive_functions() {
        let source = "function sum (:value) {\n    set next [+ value 1]\n    sum next\n}\n";
        let model = model(source);
        assert_eq!(model.symbols.len(), 3);

        let function = model
            .symbols
            .iter()
            .position(|symbol| symbol.name == "sum")
            .expect("function");
        assert_eq!(model.reference_spans(function, false).len(), 1);

        let parameter = model
            .symbols
            .iter()
            .position(|symbol| symbol.name == "value")
            .expect("parameter");
        assert_eq!(model.reference_spans(parameter, false).len(), 1);
    }

    #[test]
    fn resolves_match_captures_only_inside_the_arm() {
        let source = "set result (:good 42)\nmatch result (\n    (good :value) { value }\n)\n";
        let model = model(source);
        let capture = model
            .symbols
            .iter()
            .position(|symbol| symbol.name == "value")
            .expect("capture");
        assert_eq!(model.symbols[capture].kind, SymbolKind::PatternCapture);
        assert_eq!(model.reference_spans(capture, false).len(), 1);
    }

    #[test]
    fn lexical_shadowing_keeps_references_with_the_nearest_definition() {
        let source = "set value 1\nfunction read (:value) {\n    value\n}\nvalue\n";
        let model = model(source);
        let values = model
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, symbol)| symbol.name == "value")
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 2);
        assert_eq!(model.reference_spans(values[0].0, false).len(), 1);
        assert_eq!(model.reference_spans(values[1].0, false).len(), 1);
    }

    #[test]
    fn assignments_are_references_not_new_definitions() {
        let source = "var count 0\nlet count [+ count 1]\n";
        let model = model(source);
        assert_eq!(
            model
                .symbols
                .iter()
                .filter(|symbol| symbol.name == "count")
                .count(),
            1
        );
        let count = model
            .symbols
            .iter()
            .position(|symbol| symbol.name == "count")
            .expect("count");
        assert_eq!(model.reference_spans(count, false).len(), 2);
    }

    #[test]
    fn completion_visibility_follows_scope_and_declaration_order() {
        let source =
            "set outer 1\nfunction calculate (:input) {\n    set local input\n    local\n}\n";
        let model = model(source);
        let inside = source.find("    local\n").expect("inside function");
        let names = model
            .visible_symbols_at(inside)
            .into_iter()
            .map(|symbol| model.symbols[symbol].name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"calculate"));
        assert!(names.contains(&"input"));
        assert!(names.contains(&"local"));

        let top_level = source.len();
        let names = model
            .visible_symbols_at(top_level)
            .into_iter()
            .map(|symbol| model.symbols[symbol].name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"calculate"));
        assert!(!names.contains(&"input"));
        assert!(!names.contains(&"local"));
    }

    #[test]
    fn reports_only_unambiguous_naming_convention_violations() {
        let source = "set Point {}\nfunction parseValue (:inputValue) { inputValue }\n";
        let model = model(source);
        let issues = model.naming_issues();
        assert_eq!(issues.len(), 2);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("parseValue"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("inputValue"))
        );
        assert!(!issues.iter().any(|issue| issue.message.contains("Point")));
    }

    #[test]
    fn reports_assignment_to_a_resolved_immutable_binding() {
        let source = "set fixed 1\nlet fixed 2\nvar changing 1\nlet changing 2\n";
        let model = model(source);
        let issues = model.issues().collect::<Vec<_>>();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, IssueSeverity::Error);
        assert!(issues[0].message.contains("fixed"));
    }

    #[test]
    fn reports_only_excess_user_function_arguments() {
        let source =
            "function pair (:left :right) { (left right) }\nset partial [pair 1]\npair 1 2 3\n";
        let model = model(source);
        let issues = model.issues().collect::<Vec<_>>();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("3 were supplied"));
    }
}
