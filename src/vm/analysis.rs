use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::syntax::ast::{BlockId, Module, Name, NodeId, NodeKind, Pattern};

#[derive(Default)]
pub(super) struct ScopeAnalysis {
    declarations: Vec<Name>,
    static_blocks: HashMap<Arc<str>, BlockId>,
}

impl ScopeAnalysis {
    pub(super) fn module(module: &Module) -> Self {
        Self::run(module, module.statements.clone(), HashMap::new())
    }

    pub(super) fn function(
        module: &Module,
        body: NodeId,
        inherited_blocks: HashMap<Arc<str>, BlockId>,
    ) -> Self {
        let roots = match module.node(body).kind {
            NodeKind::Block(block) => module.block(block).statements.clone(),
            _ => vec![body],
        };
        Self::run(module, roots, inherited_blocks)
    }

    pub(super) fn block(
        module: &Module,
        block: BlockId,
        inherited_blocks: HashMap<Arc<str>, BlockId>,
    ) -> Self {
        Self::run(
            module,
            module.block(block).statements.clone(),
            inherited_blocks,
        )
    }

    pub(super) fn declarations(&self) -> &[Name] {
        &self.declarations
    }

    pub(super) fn static_block(&self, name: &str) -> Option<BlockId> {
        self.static_blocks.get(name).copied()
    }

    fn run(
        module: &Module,
        roots: Vec<NodeId>,
        inherited_blocks: HashMap<Arc<str>, BlockId>,
    ) -> Self {
        let mut walker = ScopeWalker {
            module,
            analysis: Self {
                declarations: Vec::new(),
                static_blocks: inherited_blocks,
            },
            aliases: Vec::new(),
            named_do: Vec::new(),
            visited_blocks: HashSet::new(),
            declared: HashSet::new(),
        };
        for root in roots {
            walker.visit_node(root);
        }
        loop {
            let mut changed = walker.resolve_aliases();
            let pending = std::mem::take(&mut walker.named_do);
            for name in pending {
                if let Some(block) = walker.analysis.static_block(&name)
                    && walker.visited_blocks.insert(block)
                {
                    walker.visit_block(block);
                    changed = true;
                } else if walker.analysis.static_block(&name).is_none() {
                    walker.named_do.push(name);
                }
            }
            if !changed {
                break;
            }
        }
        walker.analysis
    }
}

struct ScopeWalker<'a> {
    module: &'a Module,
    analysis: ScopeAnalysis,
    aliases: Vec<(Arc<str>, Arc<str>)>,
    named_do: Vec<Arc<str>>,
    visited_blocks: HashSet<BlockId>,
    declared: HashSet<Arc<str>>,
}

impl ScopeWalker<'_> {
    fn declare(&mut self, name: Name) {
        if self.declared.insert(name.text.clone()) {
            self.analysis.declarations.push(name);
        }
    }

    fn visit_block(&mut self, block: BlockId) {
        for statement in self.module.block(block).statements.clone() {
            self.visit_node(statement);
        }
    }

    fn visit_executable(&mut self, node: NodeId) {
        match self.module.node(node).kind {
            NodeKind::Block(block) => {
                self.visited_blocks.insert(block);
                self.visit_block(block);
            }
            _ => self.visit_node(node),
        }
    }

    fn visit_node(&mut self, node: NodeId) {
        match self.module.node(node).kind.clone() {
            NodeKind::Binding { pattern, value, .. } => {
                let mut names = Vec::new();
                capture_names(&pattern, &mut names);
                for name in names {
                    if matches!(&pattern, Pattern::Capture(_)) {
                        match self.module.node(value).kind.clone() {
                            NodeKind::Block(block) => {
                                self.analysis.static_blocks.insert(name.text.clone(), block);
                            }
                            NodeKind::Identifier(target) => {
                                self.aliases.push((name.text.clone(), target));
                            }
                            _ => {}
                        }
                    }
                    self.declare(name);
                }
                self.visit_node(value);
            }
            NodeKind::Function { name, .. } => self.declare(name),
            NodeKind::Assignment { value, .. } | NodeKind::Throw(value) => self.visit_node(value),
            NodeKind::List(nodes) => nodes.into_iter().for_each(|node| self.visit_node(node)),
            NodeKind::Member { object, .. } => self.visit_node(object),
            NodeKind::Call {
                callee, argument, ..
            } => {
                self.visit_node(callee);
                self.visit_node(argument);
            }
            NodeKind::Conditional {
                condition,
                consequent,
                alternative,
            } => {
                self.visit_node(condition);
                self.visit_executable(consequent);
                if let Some(alternative) = alternative {
                    self.visit_executable(alternative);
                }
            }
            NodeKind::Loop {
                condition, body, ..
            } => {
                self.visit_node(condition);
                self.visited_blocks.insert(body);
                self.visit_block(body);
            }
            NodeKind::Return(value) | NodeKind::Break(value) => {
                if let Some(value) = value {
                    self.visit_node(value);
                }
            }
            NodeKind::Attempt(block) => {
                self.visited_blocks.insert(block);
                self.visit_block(block);
            }
            NodeKind::Do(operand) => match self.module.node(operand).kind.clone() {
                NodeKind::Block(block) => {
                    if self.visited_blocks.insert(block) {
                        self.visit_block(block);
                    }
                }
                NodeKind::Identifier(name) => self.named_do.push(name),
                _ => {}
            },
            NodeKind::Match { value, .. } => self.visit_node(value),
            NodeKind::Unit
            | NodeKind::Boolean(_)
            | NodeKind::Integer(_)
            | NodeKind::Float(_)
            | NodeKind::String(_)
            | NodeKind::Symbol(_)
            | NodeKind::Identifier(_)
            | NodeKind::Placeholder
            | NodeKind::Block(_)
            | NodeKind::Continue
            | NodeKind::Import { .. }
            | NodeKind::NamespaceImport { .. }
            | NodeKind::New(_) => {}
        }
    }

    fn resolve_aliases(&mut self) -> bool {
        let mut changed = false;
        for (name, target) in &self.aliases {
            if self.analysis.static_blocks.contains_key(name) {
                continue;
            }
            if let Some(block) = self.analysis.static_blocks.get(target).copied() {
                self.analysis.static_blocks.insert(name.clone(), block);
                changed = true;
            }
        }
        changed
    }
}

fn capture_names(pattern: &Pattern, names: &mut Vec<Name>) {
    match pattern {
        Pattern::Capture(name) => names.push(name.clone()),
        Pattern::List(patterns) => patterns
            .iter()
            .for_each(|pattern| capture_names(pattern, names)),
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}
