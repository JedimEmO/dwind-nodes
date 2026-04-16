use dominator::{clone, events, html, with_node, Dom, EventOptions};
use futures_signals::signal::{LocalBoxSignal, Mutable, SignalExt};
use futures_signals_component_macro::component;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
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
    fn set_value(&self, val: f64) {
        self.set(val);
    }
    fn get_value(&self) -> f64 {
        self.get()
    }
}

impl<T: FloatValueWrapper + ?Sized> FloatValueWrapper for Box<T> {
    fn value_signal(&self) -> LocalBoxSignal<'static, f64> {
        (**self).value_signal()
    }
    fn set_value(&self, val: f64) {
        (**self).set_value(val)
    }
    fn get_value(&self) -> f64 {
        (**self).get_value()
    }
}

#[component(render_fn = float_input)]
struct FloatInput {
    #[default(Box::new(Mutable::new(0.0_f64)) as Box<dyn FloatValueWrapper>)]
    value: Box<dyn FloatValueWrapper + 'static>,

    #[default(false)]
    read_only: bool,

    /// Drag sensitivity: value change per pixel of horizontal mouse movement.
    #[default(0.01)]
    sensitivity: f64,
}

/// Minimum drag distance (in pixels) before switching from "click" to "scrub" mode.
const DRAG_THRESHOLD: f64 = 3.0;

/// Compact inline float input with Blender-style drag-to-scrub behavior.
///
/// - Click + drag horizontally to scrub the value (like a rotary encoder).
/// - Click without dragging to enter text edit mode.
/// - Hold Shift while dragging for fine control (10x less sensitive).
/// - Hold Ctrl while dragging to snap to integers.
pub fn float_input(props: FloatInputProps) -> Dom {
    let FloatInputProps {
        value,
        read_only,
        sensitivity,
        ..
    } = props;

    let value = std::rc::Rc::new(value);
    let editing = Mutable::new(false);

    // Drag state: (start_x, start_value)
    let drag_state: Mutable<Option<(f64, f64)>> = Mutable::new(None);
    // Track whether drag exceeded threshold (= scrub, not click)
    let did_scrub = Mutable::new(false);

    // Closures stored here so we can remove them on mouseup
    type MouseClosure = Closure<dyn FnMut(web_sys::MouseEvent)>;
    let move_closure: std::rc::Rc<std::cell::RefCell<Option<MouseClosure>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let up_closure: std::rc::Rc<std::cell::RefCell<Option<MouseClosure>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    html!("div", {
        .attr("data-port-widget", "")
        .style("width", "100%")
        .style("height", "16px")
        .style("position", "relative")
        .style("pointer-events", "auto")
        .style("user-select", "none")

        // Display mode: visible when NOT editing
        .child(html!("div", {
            .style("width", "100%")
            .style("height", "100%")
            .style("background", "rgba(0,0,0,0.3)")
            .style("border-radius", "2px")
            .style("font-size", "10px")
            .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
            .style("color", "#ccc")
            .style("display", "flex")
            .style("align-items", "center")
            .style("justify-content", "center")
            .style("box-sizing", "border-box")
            .style("overflow", "hidden")
            .style_signal("cursor", editing.signal().map(|e| if e { "text" } else { "ew-resize" }))
            .style_signal("display", editing.signal().map(|e| if e { "none" } else { "flex" }))

            // Value text
            .child(html!("span", {
                .style("pointer-events", "none")
                .text_signal(value.value_signal().map(format_value))
            }))

            // Mousedown: begin potential drag
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

                        // Document-level mousemove for drag
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
                                    let mut new_val = sv + dx * sensitivity * mult;
                                    if e.ctrl_key() || e.meta_key() {
                                        new_val = new_val.round();
                                    }
                                    value.set_value(new_val);
                                }
                            }) as Box<dyn FnMut(web_sys::MouseEvent)>)
                        };

                        // Document-level mouseup to end drag
                        let up_cb = {
                            let drag_state = drag_state.clone();
                            let did_scrub = did_scrub.clone();
                            let editing = editing.clone();
                            let move_closure = move_closure.clone();
                            let up_closure = up_closure.clone();
                            Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
                                drag_state.set(None);

                                // If we didn't scrub, it was a click — enter text edit mode
                                if !did_scrub.get() {
                                    editing.set(true);
                                }

                                // Clean up document listeners
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
                                // Drop closures
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

        // Edit mode: text input, visible only when editing
        .child(html!("input" => HtmlInputElement, {
            .attr("type", "text")
            .style("width", "100%")
            .style("height", "100%")
            .style("position", "absolute")
            .style("top", "0")
            .style("left", "0")
            .style("background", "rgba(0,0,0,0.5)")
            .style("color", "#fff")
            .style("border", "1px solid #4a9eff")
            .style("border-radius", "2px")
            .style("font-size", "10px")
            .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
            .style("padding", "0 4px")
            .style("text-align", "center")
            .style("outline", "none")
            .style("box-sizing", "border-box")
            .style("pointer-events", "auto")
            .style_signal("display", editing.signal().map(|e| if e { "block" } else { "none" }))
            .with_node!(element => {
                // When editing becomes true, focus and select all
                .future(editing.signal().for_each(clone!(element, value => move |is_editing| {
                    if is_editing {
                        element.set_value(&format_value(value.get_value()));
                        let _ = element.focus();
                        element.select();
                    }
                    async {}
                })))
                // Commit on input
                .event(clone!(element, value => move |_: events::Input| {
                    if let Ok(v) = element.value().parse::<f64>() {
                        value.set_value(v);
                    }
                }))
                // Enter / Escape to finish editing
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
                // Also stop editing on blur
                .event(clone!(editing => move |_: events::Blur| {
                    editing.set(false);
                }))
            })
        }))
    })
}

fn format_value(v: f64) -> String {
    if v == v.floor() && v.abs() < 1e6 {
        format!("{:.1}", v)
    } else {
        let s = format!("{:.3}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
