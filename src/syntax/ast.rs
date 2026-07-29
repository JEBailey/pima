use std::sync::Arc;

use crate::source::{SourceId, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Clone, Debug)]
pub struct Module {
    pub source: SourceId,
    pub statements: Vec<NodeId>,
    pub nodes: Vec<Node>,
    pub blocks: Vec<Block>,
}

impl Module {
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id.0 as usize]
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub span: Span,
    pub requirements: Vec<Arc<str>>,
    pub statements: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub span: Span,
    pub kind: NodeKind,
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Wildcard,
    Capture(Arc<str>),
    Literal(NodeId),
    List(Vec<Pattern>),
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: BlockId,
}

#[derive(Clone, Debug)]
pub enum NodeKind {
    Unit,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Arc<str>),
    Symbol(Arc<str>),
    Identifier(Arc<str>),
    Placeholder,
    List(Vec<NodeId>),
    Block(BlockId),
    Member {
        object: NodeId,
        member: Arc<str>,
    },
    Call {
        callee: NodeId,
        arguments: Vec<NodeId>,
        immediate: bool,
    },
    Binding {
        visibility: Visibility,
        mutability: BindingKind,
        pattern: Pattern,
        value: NodeId,
    },
    Assignment {
        pattern: Pattern,
        value: NodeId,
    },
    Function {
        visibility: Visibility,
        name: Arc<str>,
        parameters: Vec<Arc<str>>,
        body: BlockId,
    },
    Conditional {
        condition: NodeId,
        consequent: NodeId,
        alternative: Option<NodeId>,
    },
    Loop {
        kind: LoopKind,
        condition: NodeId,
        body: BlockId,
    },
    Return(Option<NodeId>),
    Break(Option<NodeId>),
    Continue,
    Throw(NodeId),
    Import {
        path: Arc<str>,
        alias: Option<Arc<str>>,
    },
    StaticImport {
        namespace: Arc<str>,
    },
    New(NodeId),
    Do(NodeId),
    Attempt(BlockId),
    Match {
        value: NodeId,
        arms: Vec<MatchArm>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    Immutable,
    Mutable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopKind {
    While,
    Until,
}
