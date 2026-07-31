use std::sync::Arc;

use crate::runtime::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    MakeCell {
        destination: Register,
        source: Register,
    },
    LoadCell {
        destination: Register,
        cell: Register,
    },
    StoreCell {
        cell: Register,
        source: Register,
    },
    MakeList {
        destination: Register,
        elements: Vec<Register>,
    },
    MakeNamespace {
        destination: Register,
        bindings: Vec<NamespaceBinding>,
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
    Return {
        source: Register,
    },
}

#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) constants: Vec<Value>,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) register_count: u16,
    pub(crate) functions: Vec<Function>,
    pub(crate) binding_registers: Vec<Register>,
    pub(crate) module_index: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) register_count: u16,
    pub(crate) capture_count: u16,
    pub(crate) binding_registers: Vec<Register>,
}
impl Program {
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub fn register_count(&self) -> u16 {
        self.register_count
    }
}
