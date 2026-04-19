use dominator::{clone, events, html, with_node, Dom, EventOptions};
use dwind::prelude::*;
use futures_signals::signal::{LocalBoxSignal, Mutable, SignalExt};
use futures_signals_component_macro::component;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

const FONT_STACK: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";

pub trait IntValueWrapper {
    fn value_signal(&self) -> LocalBoxSignal<'static, i64>;
    fn set_value(&self, val: i64);
    fn get_value(&self) -> i64;
}

impl IntValueWrapper for Mutable<i64> {
    fn value_signal(&self) -> LocalBoxSignal<'static, i64> {
        Box::pin(self.signal())
    }
    fn set_value(&self, val: i64) {
        self.set(val);
    }
    fn get_value(&self) -> i64 {
        self.get()
    }
}

impl<T: IntValueWrapper + ?Sized> IntValueWrapper for Box<T> {
    fn value_signal(&self) -> LocalBoxSignal<'static, i64> {
        (**self).value_signal()
    }
    fn set_value(&self, val: i64) {
        (**self).set_value(val)
    }
    fn get_value(&self) -> i64 {
        (**self).get_value()
    }
}

#[component(render_fn = int_input)]
struct IntInput {
    #[default(Box::new(Mutable::new(0_i64)) as Box<dyn IntValueWrapper>)]
    value: Box<dyn IntValueWrapper + 'static>,

    #[default(false)]
    read_only: bool,

    /// Drag sensitivity: integer units per pixel of horizontal movement.
    /// 0.25 => one unit per ~4 px, controllable for integers.
    #[default(0.25)]
    sensitivity: f64,
}

const DRAG_THRESHOLD: f64 = 3.0;

/// Compact inline integer input with Blender-style drag-to-scrub behavior.
///
/// - Click + drag horizontally to scrub the value.
/// - Click without dragging to enter text edit mode.
/// - Hold Shift while dragging for finer control (10x less sensitive).
pub fn int_input(props: IntInputProps) -> Dom {
    let IntInputProps {
        value,
        read_only,
        sensitivity,
        ..
    } = props;

    let value = std::rc::Rc::new(value);
    let editing = Mutable::new(false);

    // Drag state: (start_x, start_value)
    let drag_state: Mutable<Option<(f64, i64)>> = Mutable::new(None);
    let did_scrub = Mutable::new(false);

    type MouseClosure = Closure<dyn FnMut(web_sys::MouseEvent)>;
    let move_closure: std::rc::Rc<std::cell::RefCell<Option<MouseClosure>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let up_closure: std::rc::Rc<std::cell::RefCell<Option<MouseClosure>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    html!("div", {
        .attr("data-port-widget", "")
        .dwclass!("w-full h-4 relative pointer-events-auto")
        .style("user-select", "none")

        .child(html!("div", {
            .dwclass!("w-full h-full rounded-sm flex items-center justify-center overflow-hidden text-gray-300")
            .style("background", "rgba(0,0,0,0.3)")
            .style("font-size", "10px")
            .style("font-family", FONT_STACK)
            .style_signal("cursor", editing.signal().map(|e| if e { "text" } else { "ew-resize" }))
            .style_signal("display", editing.signal().map(|e| if e { "none" } else { "flex" }))

            .child(html!("span", {
                .style("pointer-events", "none")
                .text_signal(value.value_signal().map(|v| v.to_string()))
            }))

            .apply(|b| if read_only { b } else {
                b.event_with_options(
                    &EventOptions { preventable: true, ..EventOptions::default() },
                    clone!(value, drag_state, did_scrub, editing, move_closure, up_closure, sensitivity => move |e: events::MouseDown| {
                        if !matches!(e.button(), events::MouseButton::Left) { return; }
                        e.prevent_default();
                        e.stop_propagation();

                        let start_x = e.mouse_x() as f64;
                        let start_val = value.get_value();
                        drag_state.set(Some((start_x, start_val)));
                        did_scrub.set(false);

                        let window = web_sys::window().unwrap();
                        let document = window.document().unwrap();

                        let move_cb = {
                            let value = value.clone();
                            let drag_state = drag_state.clone();
                            let did_scrub = did_scrub.clone();
                            Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                                let (sx, sv) = match drag_state.get() {
                                    Some(s) => s,
                                    None => return,
                                };
                                let dx = e.client_x() as f64 - sx;
                                if dx.abs() >= DRAG_THRESHOLD {
                                    did_scrub.set(true);
                                }
                                if did_scrub.get() {
                                    let mult = if e.shift_key() { 0.1 } else { 1.0 };
                                    let new_val = (sv as f64 + dx * sensitivity * mult).round() as i64;
                                    value.set_value(new_val);
                                }
                            }) as Box<dyn FnMut(web_sys::MouseEvent)>)
                        };

                        let up_cb = {
                            let drag_state = drag_state.clone();
                            let did_scrub = did_scrub.clone();
                            let editing = editing.clone();
                            let move_closure = move_closure.clone();
                            let up_closure = up_closure.clone();
                            Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
                                drag_state.set(None);

                                if !did_scrub.get() {
                                    editing.set(true);
                                }

                                let doc = web_sys::window().unwrap().document().unwrap();
                                if let Some(cb) = move_closure.borrow().as_ref() {
                                    let _ = doc.remove_event_listener_with_callback(
                                        "mousemove", cb.as_ref().unchecked_ref()
                                    );
                                }
                                if let Some(cb) = up_closure.borrow().as_ref() {
                                    let _ = doc.remove_event_listener_with_callback(
                                        "mouseup", cb.as_ref().unchecked_ref()
                                    );
                                }
                                *move_closure.borrow_mut() = None;
                                *up_closure.borrow_mut() = None;
                            }) as Box<dyn FnMut(web_sys::MouseEvent)>)
                        };

                        let _ = document.add_event_listener_with_callback(
                            "mousemove", move_cb.as_ref().unchecked_ref()
                        );
                        let _ = document.add_event_listener_with_callback(
                            "mouseup", up_cb.as_ref().unchecked_ref()
                        );

                        *move_closure.borrow_mut() = Some(move_cb);
                        *up_closure.borrow_mut() = Some(up_cb);
                    })
                )
            })
        }))

        .child(html!("input" => HtmlInputElement, {
            .attr("type", "text")
            .dwclass!("w-full h-full absolute top-0 left-0 border border-picton-blue-400 rounded-sm text-white-50 px-1 text-center pointer-events-auto")
            .style("background", "rgba(0,0,0,0.5)")
            .style("font-size", "10px")
            .style("font-family", FONT_STACK)
            .style("outline", "none")
            .style_signal("display", editing.signal().map(|e| if e { "block" } else { "none" }))
            .with_node!(element => {
                .future(editing.signal().for_each(clone!(element, value => move |is_editing| {
                    if is_editing {
                        element.set_value(&value.get_value().to_string());
                        let _ = element.focus();
                        element.select();
                    }
                    async {}
                })))
                .event(clone!(element, value => move |_: events::Input| {
                    if let Ok(v) = element.value().parse::<i64>() {
                        value.set_value(v);
                    }
                }))
                .event(clone!(editing => move |e: events::KeyDown| {
                    e.stop_propagation();
                    match e.key().as_str() {
                        "Enter" | "Escape" => {
                            editing.set(false);
                            if let Some(el) = e.target() {
                                if let Ok(el) = el.dyn_into::<HtmlInputElement>() {
                                    let _ = el.blur();
                                }
                            }
                        }
                        _ => {}
                    }
                }))
                .event(clone!(editing => move |_: events::Blur| {
                    editing.set(false);
                }))
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
        let m = Mutable::new(7_i64);
        assert_eq!(<Mutable<i64> as IntValueWrapper>::get_value(&m), 7);
        <Mutable<i64> as IntValueWrapper>::set_value(&m, -42);
        assert_eq!(m.get(), -42);
    }

    #[wasm_bindgen_test]
    fn boxed_wrapper_delegates() {
        let b: Box<dyn IntValueWrapper> = Box::new(Mutable::new(3_i64));
        assert_eq!(b.get_value(), 3);
        b.set_value(11);
        assert_eq!(b.get_value(), 11);
    }
}
