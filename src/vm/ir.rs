use crate::runtime::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Register(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    Move {
        destination: Register,
        source: Register,
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
    CheckListLength {
        source: Register,
        length: u16,
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
    },
    JumpIfTrue {
        condition: Register,
        target: usize,
    },
    Call {
        destination: Register,
        function: u16,
        argument: Register,
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
}

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) register_count: u16,
    pub(crate) capture_count: u16,
}
impl Program {
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    pub fn register_count(&self) -> u16 {
        self.register_count
    }
}
