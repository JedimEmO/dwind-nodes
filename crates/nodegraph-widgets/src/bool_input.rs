use dominator::{clone, events, html, Dom, EventOptions};
use dwind::prelude::*;
use futures_signals::signal::{LocalBoxSignal, Mutable, SignalExt};
use futures_signals_component_macro::component;

pub trait BoolValueWrapper {
    fn value_signal(&self) -> LocalBoxSignal<'static, bool>;
    fn set_value(&self, val: bool);
    fn get_value(&self) -> bool;
}

impl BoolValueWrapper for Mutable<bool> {
    fn value_signal(&self) -> LocalBoxSignal<'static, bool> {
        Box::pin(self.signal())
    }
    fn set_value(&self, val: bool) {
        self.set(val);
    }
    fn get_value(&self) -> bool {
        self.get()
    }
}

impl<T: BoolValueWrapper + ?Sized> BoolValueWrapper for Box<T> {
    fn value_signal(&self) -> LocalBoxSignal<'static, bool> {
        (**self).value_signal()
    }
    fn set_value(&self, val: bool) {
        (**self).set_value(val)
    }
    fn get_value(&self) -> bool {
        (**self).get_value()
    }
}

#[component(render_fn = bool_input)]
struct BoolInput {
    #[default(Box::new(Mutable::new(false)) as Box<dyn BoolValueWrapper>)]
    value: Box<dyn BoolValueWrapper + 'static>,

    #[default(false)]
    read_only: bool,
}

/// Compact inline boolean toggle. Click flips the value.
pub fn bool_input(props: BoolInputProps) -> Dom {
    let BoolInputProps {
        value, read_only, ..
    } = props;
    let value = std::rc::Rc::new(value);

    html!("div", {
        .attr("data-port-widget", "")
        .dwclass!("w-full h-4 flex items-center justify-center pointer-events-auto")
        .style_unchecked("user-select", "none")

        .child(html!("div", {
            .dwclass!("rounded-sm border border-gray-600")
            .style("width", "12px")
            .style("height", "12px")
            .style("box-sizing", "border-box")
            .style_signal("cursor", {
                let ro = read_only;
                value.value_signal().map(move |_| if ro { "default" } else { "pointer" })
            })
            .style_signal("background", value.value_signal().map(|v| {
                if v { "var(--dw-picton-blue-400, #4fb3ff)" } else { "rgba(0,0,0,0.3)" }
            }))
            .apply(|b| if read_only { b } else {
                b.event_with_options(
                    &EventOptions { preventable: true, ..EventOptions::default() },
                    clone!(value => move |e: events::MouseDown| {
                        if !matches!(e.button(), events::MouseButton::Left) { return; }
                        e.prevent_default();
                        e.stop_propagation();
                        let cur = value.get_value();
                        value.set_value(!cur);
                    })
                )
            })
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn mutable_wrapper_round_trip() {
        let m = Mutable::new(false);
        assert!(!<Mutable<bool> as BoolValueWrapper>::get_value(&m));
        <Mutable<bool> as BoolValueWrapper>::set_value(&m, true);
        assert!(m.get());
    }

    #[wasm_bindgen_test]
    fn boxed_wrapper_delegates() {
        let b: Box<dyn BoolValueWrapper> = Box::new(Mutable::new(true));
        assert!(b.get_value());
        b.set_value(false);
        assert!(!b.get_value());
    }
}
