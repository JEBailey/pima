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
    pub requirements: Vec<Name>,
    pub statements: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name {
    pub text: Arc<str>,
    pub span: Span,
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.text.fmt(formatter)
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub span: Span,
    pub kind: NodeKind,
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Wildcard,
    Capture(Name),
    Literal(NodeId),
    List(Vec<Pattern>),
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: NodeId,
}

#[derive(Clone, Debug)]
pub struct BranchArm {
    pub condition: NodeId,
    pub result: NodeId,
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
        member: Name,
    },
    Call {
        callee: NodeId,
        argument: NodeId,
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
        name: Name,
        parameter: Pattern,
        body: NodeId,
    },
    Conditional {
        condition: NodeId,
        consequent: NodeId,
        alternative: Option<NodeId>,
    },
    Branch(Vec<BranchArm>),
    Loop {
        kind: LoopKind,
        condition: NodeId,
        body: NodeId,
    },
    Return(Option<NodeId>),
    Break(Option<NodeId>),
    Continue,
    Throw(NodeId),
    Import {
        path: Arc<str>,
        alias: Option<Arc<str>>,
    },
    NamespaceImport {
        path: Vec<Name>,
        selection: NamespaceImportSelection,
        alias: Option<Name>,
    },
    New(NodeId),
    Do(NodeId),
    Attempt(NodeId),
    Match {
        value: NodeId,
        arms: Vec<MatchArm>,
    },
}

#[derive(Clone, Debug)]
pub enum NamespaceImportSelection {
    Wildcard(Span),
    Member(Name),
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
