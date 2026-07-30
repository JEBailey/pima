macro_rules! runtime_id {
    ($name:ident, $repr:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub(crate) $repr);
    };
}

runtime_id!(SymbolId, u32);
runtime_id!(NativeFunctionId, u16);
runtime_id!(TcpListenerId, u32);
runtime_id!(TcpConnectionId, u32);
