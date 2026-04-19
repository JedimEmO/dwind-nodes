use nodegraph_core::types::socket_type::SocketType;

/// A value type that can flow across node-graph ports.
///
/// Each `ParamValue` implementor maps to exactly one `SocketType`. The
/// runtime uses that mapping to key its per-type value stores, source
/// selectors, and conversion registry.
///
/// The library provides opt-in implementations for `f64`, `i64`, `bool`,
/// `String`, and `[u8; 4]` under [`builtins`]. Applications implement this
/// trait for their domain types (e.g. `Rc<TextureBuffer>`).
pub trait ParamValue: Clone + 'static {
    const SOCKET_TYPE: SocketType;
}

/// Opt-in `ParamValue` impls for the primitive / color types the
/// texture-generator example uses. Importing this module (or using the
/// crate `prelude`) pulls in all five.
pub mod builtins {
    use super::{ParamValue, SocketType};

    impl ParamValue for f64 {
        const SOCKET_TYPE: SocketType = SocketType::Float;
    }
    impl ParamValue for i64 {
        const SOCKET_TYPE: SocketType = SocketType::Int;
    }
    impl ParamValue for bool {
        const SOCKET_TYPE: SocketType = SocketType::Bool;
    }
    impl ParamValue for String {
        const SOCKET_TYPE: SocketType = SocketType::String;
    }
    impl ParamValue for [u8; 4] {
        const SOCKET_TYPE: SocketType = SocketType::Color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use builtins::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn builtin_socket_types() {
        assert_eq!(<f64 as ParamValue>::SOCKET_TYPE, SocketType::Float);
        assert_eq!(<i64 as ParamValue>::SOCKET_TYPE, SocketType::Int);
        assert_eq!(<bool as ParamValue>::SOCKET_TYPE, SocketType::Bool);
        assert_eq!(<String as ParamValue>::SOCKET_TYPE, SocketType::String);
        assert_eq!(<[u8; 4] as ParamValue>::SOCKET_TYPE, SocketType::Color);
    }
}
