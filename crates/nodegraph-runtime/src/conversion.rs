use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use nodegraph_core::types::socket_type::SocketType;

use crate::bridge::spawn_bridge;
use crate::value::ParamValue;

/// The live bridge `Mutable<T>` (type-erased) plus an alive flag for
/// cancellation. Returned by every conversion spawner.
pub type Bridge = (Box<dyn Any>, Rc<Cell<bool>>);

/// Type-erased conversion-bridge spawner. Takes a `&dyn Any` that must
/// downcast to `Mutable<S>`, returns the bridge `Mutable<T>` + alive flag,
/// or `None` if the downcast fails.
type SpawnerFn = dyn Fn(&dyn Any) -> Option<Bridge> + 'static;

/// Registry of cross-`SocketType` conversions. When `Runtime::handle_connect`
/// sees a mismatched source/target, it asks the registry for a spawner and
/// plugs the resulting bridge `Mutable<T>` into the target's source selector.
///
/// Applications register conversions once at startup. Primitive pairs
/// (Float / Int / Bool) are the usual shape; apps can add their own for
/// custom `ParamValue` types.
pub struct ConversionRegistry {
    spawners: RefCell<HashMap<(SocketType, SocketType), Rc<SpawnerFn>>>,
}

impl Default for ConversionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversionRegistry {
    pub fn new() -> Self {
        Self {
            spawners: RefCell::new(HashMap::new()),
        }
    }

    /// Register a conversion from source type `S` to target type `T`. The
    /// `convert` closure is called once on registration-time bridging and
    /// then on every subsequent `source` emission.
    pub fn register<S, T, F>(&self, convert: F)
    where
        S: ParamValue + Copy,
        T: ParamValue,
        F: Fn(S) -> T + Clone + 'static,
    {
        let spawner = move |any_src: &dyn Any| -> Option<Bridge> {
            let src = any_src
                .downcast_ref::<futures_signals::signal::Mutable<S>>()?
                .clone();
            let (bridge, alive) = spawn_bridge(src, convert.clone());
            Some((Box::new(bridge) as Box<dyn Any>, alive))
        };
        self.spawners
            .borrow_mut()
            .insert((S::SOCKET_TYPE, T::SOCKET_TYPE), Rc::new(spawner));
    }

    /// Look up the spawner for `(src_type, tgt_type)` and invoke it on a
    /// type-erased source `Mutable<S>`. Returns the bridge + alive flag,
    /// or `None` if no conversion is registered (or if the type-erased
    /// source didn't downcast).
    pub fn spawn(
        &self,
        src_type: SocketType,
        tgt_type: SocketType,
        src: &dyn Any,
    ) -> Option<Bridge> {
        let spawner = self.spawners.borrow().get(&(src_type, tgt_type)).cloned()?;
        spawner(src)
    }

    pub fn has(&self, src_type: SocketType, tgt_type: SocketType) -> bool {
        self.spawners.borrow().contains_key(&(src_type, tgt_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_signals::signal::Mutable;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn int_to_float_registration_looks_up() {
        let reg = ConversionRegistry::new();
        reg.register::<i64, f64, _>(|i| i as f64);
        assert!(reg.has(SocketType::Int, SocketType::Float));
        assert!(!reg.has(SocketType::Float, SocketType::Int));
    }

    #[wasm_bindgen_test]
    fn spawn_returns_bridge_mutable() {
        let reg = ConversionRegistry::new();
        reg.register::<i64, f64, _>(|i| i as f64);

        let src = Mutable::new(3_i64);
        let (bridge_any, alive) = reg
            .spawn(SocketType::Int, SocketType::Float, &src)
            .expect("spawner ran");
        assert!(alive.get());
        let bridge: Box<Mutable<f64>> = bridge_any.downcast().expect("cast");
        assert_eq!(bridge.get(), 3.0);
    }

    #[wasm_bindgen_test]
    fn spawn_none_for_missing_pair() {
        let reg = ConversionRegistry::new();
        let src = Mutable::new(0_i64);
        assert!(reg
            .spawn(SocketType::Int, SocketType::Float, &src)
            .is_none());
    }

    #[wasm_bindgen_test]
    fn spawn_none_on_wrong_source_type() {
        let reg = ConversionRegistry::new();
        reg.register::<i64, f64, _>(|i| i as f64);
        let wrong_src = Mutable::new(0_f64);
        assert!(reg
            .spawn(SocketType::Int, SocketType::Float, &wrong_src)
            .is_none());
    }
}
