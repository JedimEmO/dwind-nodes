use std::cell::Cell;
use std::rc::Rc;

use futures_signals::signal::SignalExt;

use crate::computation::{NodeComputation, NodeCtx};
use crate::value::ParamValue;

/// Wire a node's param `Mutable<T>` to its output `Mutable<T>` so that
/// editing the widget on the output port drives downstream inputs.
///
/// Used by [`ConstNode`] to implement the standard const-style-node
/// pattern. Apps can also call this directly from a `NodeComputation::spawn`
/// body if they want custom behavior alongside the mirror.
pub fn spawn_const_node<T: ParamValue + Default>(
    ctx: &NodeCtx<'_>,
    alive: Rc<Cell<bool>>,
    default: T,
) {
    let out_port = match ctx.first_output_of(T::SOCKET_TYPE) {
        Some(p) => p,
        None => return,
    };
    let output = match ctx.runtime().get_output::<T>(out_port) {
        Some(o) => o,
        None => return,
    };
    let param = ctx.runtime().params().get::<T>(out_port, default);
    wasm_bindgen_futures::spawn_local(async move {
        param
            .signal_cloned()
            .for_each(move |v| {
                if alive.get() {
                    output.set(v);
                }
                async {}
            })
            .await;
    });
}

/// Ready-made `NodeComputation` for constant-style nodes (no inputs, one
/// editable output). Register once per value type; every node whose
/// `type_id` is bound to this `NodeComputation` will mirror its param
/// `Mutable<T>` onto its output.
///
/// ```ignore
/// runtime.computations().register(
///     "const_float",
///     Rc::new(ConstNode::<f64>::new(1.0)),
/// );
/// ```
pub struct ConstNode<T: ParamValue + Default> {
    default: T,
}

impl<T: ParamValue + Default> ConstNode<T> {
    pub fn new(default: T) -> Self {
        Self { default }
    }
}

impl<T: ParamValue + Default + Clone> NodeComputation for ConstNode<T> {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        spawn_const_node::<T>(ctx, alive, self.default.clone());
    }
}
