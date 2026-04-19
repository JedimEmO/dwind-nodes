use std::cell::Cell;
use std::rc::Rc;

use futures_signals::signal::{Mutable, SignalExt};

/// Spawn a tiny task that watches `source` and, on each emission, writes
/// `convert(value)` into a fresh `Mutable<T>`. Used by connection bridges
/// to plumb a cross-type source into a target's source selector.
///
/// Returns `(bridge, alive)`:
/// - `bridge` — the output `Mutable<T>` that tracks `source` through `convert`.
/// - `alive` — an alive flag the caller can flip to `false` to make the
///   task inert. Disconnection hands the flag back to the runtime, which
///   flips it; the task then stops writing (and the bridge becomes frozen).
pub fn spawn_bridge<S, T, F>(source: Mutable<S>, convert: F) -> (Mutable<T>, Rc<Cell<bool>>)
where
    S: Copy + 'static,
    T: 'static,
    F: Fn(S) -> T + 'static,
{
    let alive = Rc::new(Cell::new(true));
    let bridge = Mutable::new(convert(source.get()));
    let out = bridge.clone();
    let alive_task = alive.clone();
    wasm_bindgen_futures::spawn_local(async move {
        source
            .signal()
            .for_each(move |v| {
                if alive_task.get() {
                    out.set(convert(v));
                }
                async {}
            })
            .await;
    });
    (bridge, alive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn bridge_starts_with_converted_current_value() {
        let src = Mutable::new(7_i64);
        let (bridge, _alive) = spawn_bridge(src, |i| i as f64);
        assert_eq!(bridge.get(), 7.0);
    }

    #[wasm_bindgen_test]
    fn alive_flag_starts_true() {
        let src = Mutable::new(0_i64);
        let (_bridge, alive) = spawn_bridge(src, |i| i as f64);
        assert!(alive.get());
    }
}
