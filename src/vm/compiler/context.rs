use std::{collections::HashSet, sync::Arc};

use crate::syntax::ast::{AssignmentTarget, BlockId, Module, NodeId, NodeKind, Pattern};

/// Collect names required to execute a linked block as a compiled function.
///
/// This is deliberately separate from lexical scope analysis: blocks are
/// dynamically instantiated and therefore need an explicit execution-context
/// inventory rather than closure capture analysis.
pub(super) fn collect_block_context(
    module: &Module,
    block: BlockId,
    names: &mut HashSet<Arc<str>>,
    visited: &mut HashSet<BlockId>,
) {
    if !visited.insert(block) {
        return;
    }
    for requirement in &module.block(block).requirements {
        names.insert(requirement.name.text.clone());
    }
    for statement in &module.block(block).statements {
        collect_node_context(module, *statement, names, visited);
    }
}

fn collect_node_context(
    module: &Module,
    node: NodeId,
    names: &mut HashSet<Arc<str>>,
    visited: &mut HashSet<BlockId>,
) {
    match &module.node(node).kind {
        NodeKind::Identifier(name) => {
            names.insert(name.clone());
        }
        NodeKind::List(nodes) => {
            for node in nodes {
                collect_node_context(module, *node, names, visited);
            }
        }
        NodeKind::Block(block) => collect_block_context(module, *block, names, visited),
        NodeKind::Attempt(body) => collect_node_context(module, *body, names, visited),
        NodeKind::Member { object, .. }
        | NodeKind::New(object)
        | NodeKind::Do(object)
        | NodeKind::Remote(object)
        | NodeKind::Await(object) => collect_node_context(module, *object, names, visited),
        NodeKind::Call {
            callee, argument, ..
        } => {
            collect_node_context(module, *callee, names, visited);
            collect_node_context(module, *argument, names, visited);
        }
        NodeKind::Binding { pattern, value, .. } => {
            collect_pattern_names(pattern, names);
            collect_node_context(module, *value, names, visited);
        }
        NodeKind::Assignment { target, value } => {
            match target {
                AssignmentTarget::Pattern(pattern) => collect_pattern_names(pattern, names),
                AssignmentTarget::Member(member) => {
                    collect_node_context(module, *member, names, visited)
                }
            }
            collect_node_context(module, *value, names, visited);
        }
        NodeKind::Function {
            name,
            parameter,
            body,
            ..
        } => {
            names.insert(name.text.clone());
            collect_pattern_names(parameter, names);
            collect_node_context(module, *body, names, visited);
        }
        NodeKind::Conditional {
            condition,
            consequent,
            alternative,
        } => {
            collect_node_context(module, *condition, names, visited);
            collect_node_context(module, *consequent, names, visited);
            if let Some(alternative) = alternative {
                collect_node_context(module, *alternative, names, visited);
            }
        }
        NodeKind::Branch(arms) => {
            for arm in arms {
                collect_node_context(module, arm.condition, names, visited);
                collect_node_context(module, arm.result, names, visited);
            }
        }
        NodeKind::Loop {
            condition, body, ..
        } => {
            collect_node_context(module, *condition, names, visited);
            collect_node_context(module, *body, names, visited);
        }
        NodeKind::Return(value) | NodeKind::Break(value) => {
            if let Some(value) = value {
                collect_node_context(module, *value, names, visited);
            }
        }
        NodeKind::Throw(value) => collect_node_context(module, *value, names, visited),
        NodeKind::Match { value, arms } => {
            collect_node_context(module, *value, names, visited);
            for arm in arms {
                collect_pattern_names(&arm.pattern, names);
                collect_node_context(module, arm.body, names, visited);
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
        | NodeKind::NamespaceImport { .. } => {}
    }
}

pub(super) fn collect_pattern_names(pattern: &Pattern, names: &mut HashSet<Arc<str>>) {
    match pattern {
        Pattern::Capture(name) => {
            names.insert(name.text.clone());
        }
        Pattern::List(patterns) => {
            for pattern in patterns {
                collect_pattern_names(pattern, names);
            }
        }
        Pattern::Literal(_) | Pattern::Wildcard => {}
    }
}
