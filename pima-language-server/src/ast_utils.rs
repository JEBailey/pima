use pima::syntax::ast::{BlockId, Module, Name, NodeId, NodeKind, Pattern};

pub fn pattern_captures(pattern: &Pattern) -> Vec<&Name> {
    match pattern {
        Pattern::Capture(name) => vec![name],
        Pattern::List(elements) => elements.iter().flat_map(pattern_captures).collect(),
        Pattern::Wildcard | Pattern::Literal(_) => Vec::new(),
    }
}

pub fn namespace_block(module: &Module, value: NodeId) -> Option<BlockId> {
    match &module.node(value).kind {
        NodeKind::Block(block) => Some(*block),
        NodeKind::New(operand) => match module.node(*operand).kind {
            NodeKind::Block(block) => Some(block),
            _ => None,
        },
        _ => None,
    }
}

pub fn parameter_list(parameters: &[Name]) -> String {
    format!(
        "({})",
        parameters
            .iter()
            .map(|parameter| parameter.text.as_ref())
            .collect::<Vec<_>>()
            .join(", ")
    )
}
