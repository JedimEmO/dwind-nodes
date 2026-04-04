use wasm_bindgen::JsCast;
use dominator::{clone, events, html, with_node, Dom};
use futures_signals::signal::{LocalBoxSignal, Mutable, SignalExt};
use futures_signals_component_macro::component;
use web_sys::HtmlInputElement;

/// Trait for reading/writing a float value reactively.
pub trait FloatValueWrapper {
    fn value_signal(&self) -> LocalBoxSignal<'static, f64>;
    fn set_value(&self, val: f64);
    fn get_value(&self) -> f64;
}

impl FloatValueWrapper for Mutable<f64> {
    fn value_signal(&self) -> LocalBoxSignal<'static, f64> {
        Box::pin(self.signal())
    }
    fn set_value(&self, val: f64) { self.set(val); }
    fn get_value(&self) -> f64 { self.get() }
}

impl<T: FloatValueWrapper + ?Sized> FloatValueWrapper for Box<T> {
    fn value_signal(&self) -> LocalBoxSignal<'static, f64> { (**self).value_signal() }
    fn set_value(&self, val: f64) { (**self).set_value(val) }
    fn get_value(&self) -> f64 { (**self).get_value() }
}

#[component(render_fn = float_input)]
struct FloatInput {
    #[default(Box::new(Mutable::new(0.0_f64)) as Box<dyn FloatValueWrapper>)]
    value: Box<dyn FloatValueWrapper + 'static>,

    #[default(false)]
    read_only: bool,
}

/// Compact inline float input for node graph port rows.
pub fn float_input(props: FloatInputProps) -> Dom {
    let FloatInputProps {
        value,
        read_only,
        ..
    } = props;

    html!("input" => HtmlInputElement, {
        .attr("type", "text")
        .style("width", "100%")
        .style("height", "16px")
        .style("background", "rgba(0,0,0,0.3)")
        .style("color", "#ccc")
        .style("border", "none")
        .style("border-radius", "2px")
        .style("font-size", "10px")
        .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
        .style("padding", "0 4px")
        .style("text-align", "center")
        .style("outline", "none")
        .style("box-sizing", "border-box")
        .style("pointer-events", "auto")
        .apply(|b| if read_only {
            b.attr("readonly", "readonly")
             .style("cursor", "default")
        } else {
            b.style("cursor", "text")
        })
        .with_node!(element => {
            .future(value.value_signal().for_each(clone!(element => move |v| {
                element.set_value(&format!("{}", v));
                async {}
            })))
            .event(clone!(element => move |_: events::Input| {
                if let Ok(v) = element.value().parse::<f64>() {
                    value.set_value(v);
                }
            }))
        })
        .event(|e: events::KeyDown| {
            e.stop_propagation();
            if e.key() == "Escape" {
                if let Some(el) = e.target() {
                    if let Ok(el) = el.dyn_into::<HtmlInputElement>() {
                        el.blur().ok();
                    }
                }
            }
        })
    })
}
