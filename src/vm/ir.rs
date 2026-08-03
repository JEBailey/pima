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
    pub mutable: bool,
}

#[derive(Clone, Debug)]
pub struct RemoteContextBinding {
    pub name: Arc<str>,
    pub source: Register,
    pub mode: crate::runtime::ContextTransferMode,
    pub move_target: Option<Register>,
}

/// Call operands retained in VM registers until the call executes. A packed
/// argument is materialized only at the language boundary, eliminating the
/// former `MakeArguments` temporary register and dispatch.
#[derive(Clone, Debug)]
pub enum CallArgument {
    Value(Register),
    Pack(Vec<Register>),
}

#[derive(Clone, Debug, Default)]
pub struct MemberCache(std::cell::Cell<Option<usize>>);

impl MemberCache {
    pub(crate) fn get(&self) -> Option<usize> {
        self.0.get()
    }

    pub(crate) fn set(&self, index: usize) {
        self.0.set(Some(index));
    }
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

/// Optimizer-facing register IR and the VM's executable instruction format.
///
/// The explicit one-byte tag keeps the dense instruction stream compact while
/// preserving the structured operands used by compiler passes and diagnostics.
/// Keep the layout test below when adding variants: a large increase is a sign
/// that an uncommon operand belongs in a side table rather than the hot stream.
#[derive(Clone, Debug)]
#[repr(u8)]
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
        construction: Option<Register>,
    },
    InitializeConstruction {
        binding: Register,
    },
    RecordConstructionFailure {
        binding: Register,
        error: Register,
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
    BindImport {
        binding: Register,
        source: Register,
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
        self_binding: Option<Register>,
    },
    MakeRemoteNamespace {
        destination: Register,
        /// Remote construction metadata is large and cold. Boxing it prevents
        /// this rare variant from determining the size of every instruction.
        blueprint: Box<RemoteBlueprint>,
        context: Vec<RemoteContextBinding>,
    },
    AwaitTask {
        destination: Register,
        task: Register,
    },
    LoadMember {
        destination: Register,
        namespace: Register,
        name: Arc<str>,
        allow_private: bool,
        cache: MemberCache,
    },
    /// Member read whose receiver is compiler-proven to be a local namespace.
    LoadNamespaceMember {
        destination: Register,
        namespace: Register,
        name: Arc<str>,
        allow_private: bool,
        cache: MemberCache,
    },
    StoreMember {
        namespace: Register,
        source: Register,
        name: Arc<str>,
        allow_private: bool,
        cache: MemberCache,
    },
    CheckMemberWritable {
        namespace: Register,
        name: Arc<str>,
        allow_private: bool,
        cache: MemberCache,
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
    CallClosure {
        destination: Register,
        callee: Register,
        argument: CallArgument,
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
        construction: Option<Register>,
    },
    CallDynamic {
        destination: Register,
        callee: Register,
        argument: CallArgument,
        command: bool,
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
    pub(crate) fn remap_registers(&mut self, mapping: &[Register]) {
        let map = |register: &mut Register| *register = mapping[register.0 as usize];
        let map_argument = |argument: &mut CallArgument| match argument {
            CallArgument::Value(register) => map(register),
            CallArgument::Pack(registers) => registers.iter_mut().for_each(map),
        };
        match self {
            Self::LoadConstant { destination, .. } | Self::LoadSymbol { destination, .. } => {
                map(destination)
            }
            Self::MakeBlock {
                destination,
                construction,
                ..
            } => {
                map(destination);
                construction.iter_mut().for_each(map);
            }
            Self::InitializeConstruction { binding } | Self::CheckWritable { binding, .. } => {
                map(binding)
            }
            Self::RecordConstructionFailure { binding, error } => {
                map(binding);
                map(error);
            }
            Self::Move {
                destination,
                source,
            }
            | Self::BindImport {
                binding: destination,
                source,
                ..
            }
            | Self::LoadBinding {
                destination,
                binding: source,
                ..
            }
            | Self::StoreBinding {
                binding: destination,
                source,
                ..
            }
            | Self::ListGet {
                destination,
                source,
                ..
            } => {
                map(destination);
                map(source);
            }
            Self::Bind {
                binding, source, ..
            } => {
                map(binding);
                map(source);
            }
            Self::MakeList {
                destination,
                elements,
            } => {
                map(destination);
                elements.iter_mut().for_each(map);
            }
            Self::MergeNamespaceTypes {
                destination,
                sources,
            } => {
                map(destination);
                sources.iter_mut().for_each(map);
            }
            Self::MakeNamespace {
                destination,
                bindings,
                self_binding,
            } => {
                map(destination);
                bindings
                    .iter_mut()
                    .for_each(|binding| map(&mut binding.source));
                self_binding.iter_mut().for_each(map);
            }
            Self::MakeRemoteNamespace {
                destination,
                context,
                ..
            } => {
                map(destination);
                context.iter_mut().for_each(|binding| {
                    map(&mut binding.source);
                    binding.move_target.iter_mut().for_each(map);
                });
            }
            Self::AwaitTask { destination, task } => {
                map(destination);
                map(task);
            }
            Self::LoadMember {
                destination,
                namespace,
                ..
            }
            | Self::LoadNamespaceMember {
                destination,
                namespace,
                ..
            } => {
                map(destination);
                map(namespace);
            }
            Self::StoreMember {
                namespace, source, ..
            } => {
                map(namespace);
                map(source);
            }
            Self::CheckMemberWritable { namespace, .. }
            | Self::CheckListLength {
                source: namespace, ..
            }
            | Self::JumpIfNotListLength {
                source: namespace, ..
            }
            | Self::JumpIfNotBlock {
                source: namespace, ..
            }
            | Self::JumpIfFalse {
                condition: namespace,
                ..
            }
            | Self::JumpIfTrue {
                condition: namespace,
                ..
            }
            | Self::Throw { source: namespace }
            | Self::Return { source: namespace } => map(namespace),
            Self::JumpIfNotEqual { left, right, .. } => {
                map(left);
                map(right);
            }
            Self::CallPrimitive {
                destination,
                arguments,
                ..
            } => {
                map(destination);
                arguments.iter_mut().for_each(map);
            }
            Self::CallClosure {
                destination,
                callee,
                argument,
            }
            | Self::CallDynamic {
                destination,
                callee,
                argument,
                ..
            } => {
                map(destination);
                map(callee);
                map_argument(argument);
            }
            Self::MakeClosure {
                destination,
                captures,
                construction,
                ..
            } => {
                map(destination);
                captures.iter_mut().for_each(map);
                construction.iter_mut().for_each(map);
            }
            Self::DoDynamic {
                destination,
                block,
                context,
            } => {
                map(destination);
                map(block);
                context.iter_mut().for_each(|(_, register)| map(register));
            }
            Self::BeginAttempt { destination, .. } => map(destination),
            Self::PublishExports { bindings } => bindings
                .iter_mut()
                .for_each(|binding| map(&mut binding.source)),
            Self::Jump { .. } | Self::EndAttempt | Self::RaiseTyped { .. } => {}
        }
    }

    pub(crate) fn visit_registers(&self, mut visit: impl FnMut(Register)) {
        let mut one = |register: &Register| visit(*register);
        match self {
            Self::LoadConstant { destination, .. } | Self::LoadSymbol { destination, .. } => {
                one(destination)
            }
            Self::MakeBlock {
                destination,
                construction,
                ..
            } => {
                one(destination);
                construction.iter().for_each(&mut one);
            }
            Self::InitializeConstruction { binding } | Self::CheckWritable { binding, .. } => {
                one(binding)
            }
            Self::RecordConstructionFailure { binding, error } => {
                one(binding);
                one(error);
            }
            Self::Move {
                destination,
                source,
            }
            | Self::BindImport {
                binding: destination,
                source,
                ..
            }
            | Self::LoadBinding {
                destination,
                binding: source,
                ..
            }
            | Self::StoreBinding {
                binding: destination,
                source,
                ..
            }
            | Self::ListGet {
                destination,
                source,
                ..
            } => {
                one(destination);
                one(source);
            }
            Self::Bind {
                binding, source, ..
            } => {
                one(binding);
                one(source);
            }
            Self::MakeList {
                destination,
                elements,
            } => {
                one(destination);
                elements.iter().for_each(&mut one);
            }
            Self::MergeNamespaceTypes {
                destination,
                sources,
            } => {
                one(destination);
                sources.iter().for_each(&mut one);
            }
            Self::MakeNamespace {
                destination,
                bindings,
                self_binding,
            } => {
                one(destination);
                bindings.iter().for_each(|binding| one(&binding.source));
                self_binding.iter().for_each(&mut one);
            }
            Self::MakeRemoteNamespace {
                destination,
                context,
                ..
            } => {
                one(destination);
                context.iter().for_each(|binding| {
                    one(&binding.source);
                    binding.move_target.iter().for_each(&mut one);
                });
            }
            Self::AwaitTask { destination, task } => {
                one(destination);
                one(task);
            }
            Self::LoadMember {
                destination,
                namespace,
                ..
            }
            | Self::LoadNamespaceMember {
                destination,
                namespace,
                ..
            } => {
                one(destination);
                one(namespace);
            }
            Self::StoreMember {
                namespace, source, ..
            } => {
                one(namespace);
                one(source);
            }
            Self::CheckMemberWritable { namespace, .. }
            | Self::CheckListLength {
                source: namespace, ..
            }
            | Self::JumpIfNotListLength {
                source: namespace, ..
            }
            | Self::JumpIfNotBlock {
                source: namespace, ..
            }
            | Self::JumpIfFalse {
                condition: namespace,
                ..
            }
            | Self::JumpIfTrue {
                condition: namespace,
                ..
            }
            | Self::Throw { source: namespace }
            | Self::Return { source: namespace } => one(namespace),
            Self::JumpIfNotEqual { left, right, .. } => {
                one(left);
                one(right);
            }
            Self::CallPrimitive {
                destination,
                arguments,
                ..
            } => {
                one(destination);
                arguments.iter().for_each(&mut one);
            }
            Self::CallClosure {
                destination,
                callee,
                argument,
            } => {
                one(destination);
                one(callee);
                match argument {
                    CallArgument::Value(register) => one(register),
                    CallArgument::Pack(registers) => registers.iter().for_each(&mut one),
                }
            }
            Self::MakeClosure {
                destination,
                captures,
                construction,
                ..
            } => {
                one(destination);
                captures.iter().for_each(&mut one);
                construction.iter().for_each(&mut one);
            }
            Self::CallDynamic {
                destination,
                callee,
                argument,
                ..
            } => {
                one(destination);
                one(callee);
                match argument {
                    CallArgument::Value(register) => one(register),
                    CallArgument::Pack(registers) => registers.iter().for_each(&mut one),
                }
            }
            Self::DoDynamic {
                destination,
                block,
                context,
            } => {
                one(destination);
                one(block);
                context.iter().for_each(|(_, register)| one(register));
            }
            Self::BeginAttempt { destination, .. } => one(destination),
            Self::PublishExports { bindings } => {
                bindings.iter().for_each(|binding| one(&binding.source))
            }
            Self::Jump { .. } | Self::EndAttempt | Self::RaiseTyped { .. } => {}
        }
    }

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

#[cfg(test)]
mod layout_tests {
    use super::Instruction;

    #[test]
    fn executable_instruction_remains_compact() {
        assert!(
            std::mem::size_of::<Instruction>() <= 48,
            "Instruction grew to {} bytes; move uncommon operands to a side table",
            std::mem::size_of::<Instruction>()
        );
    }
}
