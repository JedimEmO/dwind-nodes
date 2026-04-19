use dominator::{clone, events, html, Dom};
use dwind::prelude::*;
use futures_signals::signal::{LocalBoxSignal, Mutable, SignalExt};
use futures_signals_component_macro::component;
use wasm_bindgen::JsCast;

pub trait ColorValueWrapper {
    fn value_signal(&self) -> LocalBoxSignal<'static, [u8; 4]>;
    fn set_value(&self, val: [u8; 4]);
    fn get_value(&self) -> [u8; 4];
}

impl ColorValueWrapper for Mutable<[u8; 4]> {
    fn value_signal(&self) -> LocalBoxSignal<'static, [u8; 4]> {
        Box::pin(self.signal())
    }
    fn set_value(&self, val: [u8; 4]) {
        self.set(val);
    }
    fn get_value(&self) -> [u8; 4] {
        self.get()
    }
}

impl<T: ColorValueWrapper + ?Sized> ColorValueWrapper for Box<T> {
    fn value_signal(&self) -> LocalBoxSignal<'static, [u8; 4]> {
        (**self).value_signal()
    }
    fn set_value(&self, val: [u8; 4]) {
        (**self).set_value(val)
    }
    fn get_value(&self) -> [u8; 4] {
        (**self).get_value()
    }
}

#[component(render_fn = color_input)]
struct ColorInput {
    #[default(Box::new(Mutable::new([200u8, 200, 200, 255])) as Box<dyn ColorValueWrapper>)]
    value: Box<dyn ColorValueWrapper + 'static>,

    #[default(false)]
    read_only: bool,
}

/// Compact inline color swatch; clicking opens the browser's native color picker.
pub fn color_input(props: ColorInputProps) -> Dom {
    let ColorInputProps {
        value, read_only, ..
    } = props;
    let value = std::rc::Rc::new(value);

    html!("div", {
        .attr("data-port-widget", "")
        .dwclass!("relative w-full h-4 pointer-events-auto")

        .child(html!("div", {
            .dwclass!("w-full h-full rounded-sm border border-gray-600")
            .style("cursor", if read_only { "default" } else { "pointer" })
            .style_signal("background", value.value_signal().map(|c| {
                format!("rgb({},{},{})", c[0], c[1], c[2])
            }))
        }))

        .apply(|b| if read_only { b } else {
            b.child(html!("input" => web_sys::HtmlInputElement, {
                .attr("type", "color")
                .dwclass!("absolute top-0 left-0 w-full h-full cursor-pointer")
                .style("opacity", "0")
                .attr_signal("value", value.value_signal().map(rgba_to_hex))
                .event(clone!(value => move |e: events::Input| {
                    let target: web_sys::HtmlInputElement = e.target().unwrap().unchecked_into();
                    let hex = target.value();
                    value.set_value(hex_to_rgba(&hex));
                }))
            }))
        })
    })
}

pub fn hex_to_rgba(hex: &str) -> [u8; 4] {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(200);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(200);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(200);
        [r, g, b, 255]
    } else {
        [200, 200, 200, 255]
    }
}

pub fn rgba_to_hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn hex_rgba_roundtrip() {
        assert_eq!(hex_to_rgba("#8b6914"), [139, 105, 20, 255]);
        let hex = rgba_to_hex([139, 105, 20, 255]);
        assert_eq!(hex_to_rgba(&hex), [139, 105, 20, 255]);
    }

    #[wasm_bindgen_test]
    fn hex_rgba_short_is_fallback() {
        assert_eq!(hex_to_rgba("#abc"), [200, 200, 200, 255]);
    }

    #[wasm_bindgen_test]
    fn mutable_wrapper_round_trip() {
        let m: Mutable<[u8; 4]> = Mutable::new([0, 0, 0, 255]);
        assert_eq!(
            <Mutable<[u8; 4]> as ColorValueWrapper>::get_value(&m),
            [0, 0, 0, 255]
        );
        <Mutable<[u8; 4]> as ColorValueWrapper>::set_value(&m, [1, 2, 3, 255]);
        assert_eq!(m.get(), [1, 2, 3, 255]);
    }
}
