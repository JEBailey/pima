use std::sync::Arc;

use crate::runtime::{RemoteBlueprint, Value};
use crate::source::Span;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Register(pub u16);

#[derive(Clone, Debug)]
pub struct NamespaceBinding {
    pub name: Arc<str>,
    pub source: Register,
    pub public: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Primitive {
    Add,
    Subtract,
    Multiply,
    Divide,
    IntegerDivide,
    Modulo,
    LessThan,
    GreaterThan,
    Equal,
}

#[derive(Clone, Debug)]
pub enum Instruction {
    LoadConstant {
        destination: Register,
        constant: u16,
    },
    LoadSymbol {
        destination: Register,
        name: Arc<str>,
    },
    MakeBlock {
        destination: Register,
        block: u32,
        function: u16,
        context: Vec<Arc<str>>,
    },
    Move {
        destination: Register,
        source: Register,
    },
    Bind {
        binding: Register,
        source: Register,
        mutable: bool,
        name: Arc<str>,
    },
    LoadBinding {
        destination: Register,
        binding: Register,
        name: Arc<str>,
    },
    StoreBinding {
        binding: Register,
        source: Register,
        name: Arc<str>,
    },
    CheckWritable {
        binding: Register,
        name: Arc<str>,
    },
    MakeList {
        destination: Register,
        elements: Vec<Register>,
    },
    MergeNamespaceTypes {
        destination: Register,
        sources: Vec<Register>,
    },
    MakeNamespace {
        destination: Register,
        bindings: Vec<NamespaceBinding>,
    },
    MakeRemoteNamespace {
        destination: Register,
        blueprint: RemoteBlueprint,
        context: Vec<(Arc<str>, Register)>,
    },
    AwaitTask {
        destination: Register,
        task: Register,
    },
    LoadMember {
        destination: Register,
        namespace: Register,
        name: Arc<str>,
    },
    CheckListLength {
        source: Register,
        length: u16,
        message: Arc<str>,
    },
    JumpIfNotListLength {
        source: Register,
        length: u16,
        target: usize,
    },
    JumpIfNotEqual {
        left: Register,
        right: Register,
        target: usize,
    },
    JumpIfNotBlock {
        source: Register,
        module: usize,
        block: u32,
        target: usize,
    },
    ListGet {
        destination: Register,
        source: Register,
        index: u16,
    },
    CallPrimitive {
        destination: Register,
        primitive: Primitive,
        arguments: Vec<Register>,
    },
    Jump {
        target: usize,
    },
    JumpIfFalse {
        condition: Register,
        target: usize,
        message: Arc<str>,
    },
    JumpIfTrue {
        condition: Register,
        target: usize,
        message: Arc<str>,
    },
    MakeClosure {
        destination: Register,
        function: u16,
        captures: Vec<Register>,
    },
    CallDynamic {
        destination: Register,
        callee: Register,
        argument: Register,
    },
    DoDynamic {
        destination: Register,
        block: Register,
        context: Vec<(Arc<str>, Register)>,
    },
    BeginAttempt {
        destination: Register,
        catch_target: usize,
    },
    EndAttempt,
    Throw {
        source: Register,
    },
    RaiseTyped {
        types: Vec<Arc<str>>,
        message: Arc<str>,
    },
    PublishExports {
        bindings: Vec<NamespaceBinding>,
    },
    Return {
        source: Register,
    },
}

impl Instruction {
    pub(crate) fn target(&self) -> Option<usize> {
        match self {
            Self::Jump { target }
            | Self::JumpIfFalse { target, .. }
            | Self::JumpIfTrue { target, .. }
            | Self::JumpIfNotListLength { target, .. }
            | Self::JumpIfNotEqual { target, .. }
            | Self::JumpIfNotBlock { target, .. } => Some(*target),
            Self::BeginAttempt { catch_target, .. } => Some(*catch_target),
            _ => None,
        }
    }

    pub(crate) fn set_target(&mut self, new_target: usize) {
        match self {
            Self::Jump { target }
            | Self::JumpIfFalse { target, .. }
            | Self::JumpIfTrue { target, .. }
            | Self::JumpIfNotListLength { target, .. }
            | Self::JumpIfNotEqual { target, .. }
            | Self::JumpIfNotBlock { target, .. } => *target = new_target,
            Self::BeginAttempt { catch_target, .. } => *catch_target = new_target,
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) id: u64,
    pub(crate) constants: Vec<Value>,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) instruction_spans: Vec<Option<Span>>,
    pub(crate) register_count: u16,
    pub(crate) functions: Vec<Function>,
    pub(crate) binding_registers: Vec<Register>,
    pub(crate) initial_bindings: Vec<(Register, Value)>,
    pub(crate) module_index: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub(crate) name: Arc<str>,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) instruction_spans: Vec<Option<Span>>,
    pub(crate) register_count: u16,
    pub(crate) capture_count: u16,
    pub(crate) parameter_count: Option<u16>,
    pub(crate) binding_registers: Vec<Register>,
}
impl Program {
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub fn register_count(&self) -> u16 {
        self.register_count
    }

    /// Visits the module body and every function body in execution order.
    ///
    /// Rewriting passes must keep instructions and source spans aligned and
    /// must leave all control-flow targets valid when the callback returns.
    pub fn visit_instruction_sequences_mut(
        &mut self,
        mut visitor: impl FnMut(&mut Vec<Instruction>, &mut Vec<Option<Span>>),
    ) {
        visitor(&mut self.instructions, &mut self.instruction_spans);
        for function in &mut self.functions {
            visitor(&mut function.instructions, &mut function.instruction_spans);
        }
    }
}
