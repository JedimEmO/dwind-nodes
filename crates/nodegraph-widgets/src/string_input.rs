use dominator::{clone, events, html, with_node, Dom};
use dwind::prelude::*;
use futures_signals::signal::{LocalBoxSignal, Mutable, SignalExt};
use futures_signals_component_macro::component;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

const FONT_STACK: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";

pub trait StringValueWrapper {
    fn value_signal(&self) -> LocalBoxSignal<'static, String>;
    fn set_value(&self, val: String);
    fn get_value(&self) -> String;
}

impl StringValueWrapper for Mutable<String> {
    fn value_signal(&self) -> LocalBoxSignal<'static, String> {
        Box::pin(self.signal_cloned())
    }
    fn set_value(&self, val: String) {
        self.set(val);
    }
    fn get_value(&self) -> String {
        self.get_cloned()
    }
}

impl<T: StringValueWrapper + ?Sized> StringValueWrapper for Box<T> {
    fn value_signal(&self) -> LocalBoxSignal<'static, String> {
        (**self).value_signal()
    }
    fn set_value(&self, val: String) {
        (**self).set_value(val)
    }
    fn get_value(&self) -> String {
        (**self).get_value()
    }
}

#[component(render_fn = string_input)]
struct StringInput {
    #[default(Box::new(Mutable::new(String::new())) as Box<dyn StringValueWrapper>)]
    value: Box<dyn StringValueWrapper + 'static>,

    #[default(false)]
    read_only: bool,
}

/// Compact inline string input. Always editable; value is live-synced on every keystroke.
pub fn string_input(props: StringInputProps) -> Dom {
    let StringInputProps {
        value, read_only, ..
    } = props;
    let value = std::rc::Rc::new(value);

    html!("input" => HtmlInputElement, {
        .attr("data-port-widget", "")
        .attr("type", "text")
        .apply(|b| if read_only { b.attr("readonly", "") } else { b })
        .dwclass!("w-full h-4 rounded-sm text-white-50 px-1 pointer-events-auto")
        .style("background", "rgba(0,0,0,0.3)")
        .style("border", "1px solid transparent")
        .style("font-size", "10px")
        .style("font-family", FONT_STACK)
        .style("outline", "none")
        .style("box-sizing", "border-box")
        .with_node!(element => {
            .future(value.value_signal().for_each(clone!(element => move |v| {
                // Avoid clobbering the user's in-flight typing by only writing when different.
                if element.value() != v {
                    element.set_value(&v);
                }
                async {}
            })))
            .event(clone!(value => move |_: events::Input| {
                value.set_value(element.value());
            }))
            .event(move |e: events::KeyDown| {
                // Prevent viewport shortcuts from firing while typing.
                e.stop_propagation();
                if matches!(e.key().as_str(), "Enter" | "Escape") {
                    if let Some(el) = e.target() {
                        if let Ok(el) = el.dyn_into::<HtmlInputElement>() {
                            let _ = el.blur();
                        }
                    }
                }
            })
            .event(|e: events::MouseDown| {
                // Don't let node-drag / pan logic consume the click.
                e.stop_propagation();
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn mutable_wrapper_round_trip() {
        let m = Mutable::new(String::from("hello"));
        assert_eq!(
            <Mutable<String> as StringValueWrapper>::get_value(&m),
            "hello"
        );
        <Mutable<String> as StringValueWrapper>::set_value(&m, "world".into());
        assert_eq!(m.get_cloned(), "world");
    }

    #[wasm_bindgen_test]
    fn boxed_wrapper_delegates() {
        let b: Box<dyn StringValueWrapper> = Box::new(Mutable::new(String::from("a")));
        assert_eq!(b.get_value(), "a");
        b.set_value("b".into());
        assert_eq!(b.get_value(), "b");
    }
}
