use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dominator::{html, events, clone};
use futures_signals::signal::{Mutable, SignalExt};
use wasm_bindgen::JsCast;

use nodegraph_core::{EntityId, PortDirection, SocketType};
use nodegraph_render::GraphSignals;
use nodegraph_widgets::float_input::{float_input, FloatInputProps};

pub struct ParamStore {
    floats: RefCell<HashMap<EntityId, Mutable<f64>>>,
    colors: RefCell<HashMap<EntityId, Mutable<[u8; 4]>>>,
    /// Callback fired whenever any param value changes. Set once after construction.
    on_change: RefCell<Option<Rc<dyn Fn()>>>,
}

impl ParamStore {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            floats: RefCell::new(HashMap::new()),
            colors: RefCell::new(HashMap::new()),
            on_change: RefCell::new(None),
        })
    }

    /// Set the callback that fires on any param change.
    pub fn set_on_change(&self, cb: Rc<dyn Fn()>) {
        *self.on_change.borrow_mut() = Some(cb);
    }

    pub fn get_float(&self, port_id: EntityId, default: f64) -> Mutable<f64> {
        Self::get_or_create(&self.floats, port_id, default, &self.on_change)
    }

    pub fn get_color(&self, port_id: EntityId, default: [u8; 4]) -> Mutable<[u8; 4]> {
        Self::get_or_create(&self.colors, port_id, default, &self.on_change)
    }

    fn get_or_create<T: Copy + 'static>(
        map: &RefCell<HashMap<EntityId, Mutable<T>>>,
        port_id: EntityId,
        default: T,
        on_change: &RefCell<Option<Rc<dyn Fn()>>>,
    ) -> Mutable<T> {
        let mut map = map.borrow_mut();
        if let Some(m) = map.get(&port_id) {
            return m.clone();
        }
        let m = Mutable::new(default);
        map.insert(port_id, m.clone());
        if let Some(cb) = on_change.borrow().clone() {
            let sig = m.clone();
            wasm_bindgen_futures::spawn_local(async move {
                sig.signal().for_each(move |_| {
                    cb();
                    async {}
                }).await;
            });
        }
        m
    }
}

/// Default float values per node type + port label.
pub fn default_float(type_id: &str, label: &str) -> f64 {
    match (type_id, label) {
        ("checker", "Size") => 4.0,
        ("noise", "Scale") => 5.0,
        ("noise", "Seed") => 1.0,
        ("brick", "Rows") => 4.0,
        ("mix", "Factor") => 0.5,
        ("brightness_contrast", "Brightness") => 0.0,
        ("brightness_contrast", "Contrast") => 0.0,
        ("threshold", "Level") => 0.5,
        _ => 0.0,
    }
}

/// Default color values per node type + port label.
pub fn default_color(type_id: &str, label: &str) -> [u8; 4] {
    match (type_id, label) {
        ("solid_color", "Color") => [139, 105, 20, 255],     // brown
        ("checker", "Color A") => [180, 180, 180, 255],       // light gray
        ("checker", "Color B") => [80, 80, 80, 255],          // dark gray
        ("gradient", "Color A") => [20, 20, 60, 255],         // dark blue
        ("gradient", "Color B") => [200, 180, 140, 255],      // sand
        ("brick", "Mortar") => [120, 120, 120, 255],           // gray mortar
        ("brick", "Brick") => [160, 80, 60, 255],              // red brick
        ("colorize", "Tint") => [139, 105, 20, 255],           // brown
        _ => [200, 200, 200, 255],
    }
}

fn hex_to_rgba(hex: &str) -> [u8; 4] {
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

fn rgba_to_hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Build the port_widget callback for the texture generator.
pub fn make_port_widget(params: &Rc<ParamStore>) -> Rc<dyn Fn(EntityId, EntityId, SocketType, PortDirection, &str, bool, &Rc<GraphSignals>) -> Option<dominator::Dom>> {
    let params = params.clone();
    Rc::new(move |_node_id, port_id, socket_type, port_dir, type_id, is_connected, _gs| {
        // Solid Color: always show color picker on its output port
        if type_id == "solid_color" && port_dir == PortDirection::Output {
            let label = _gs.with_graph(|g| {
                g.world.get::<nodegraph_core::graph::port::PortLabel>(port_id)
                    .map(|l| l.0.clone())
                    .unwrap_or_default()
            });
            let default = default_color(type_id, &label);
            let mutable = params.get_color(port_id, default);
            return Some(color_picker(mutable));
        }

        // Only show widgets on disconnected input ports
        if port_dir != PortDirection::Input || is_connected {
            return None;
        }

        match socket_type {
            SocketType::Float => {
                let label = _gs.with_graph(|g| {
                    g.world.get::<nodegraph_core::graph::port::PortLabel>(port_id)
                        .map(|l| l.0.clone())
                        .unwrap_or_default()
                });
                let default = default_float(type_id, &label);
                let mutable = params.get_float(port_id, default);
                Some(float_input(FloatInputProps::new()
                    .value(Box::new(mutable) as Box<dyn nodegraph_widgets::FloatValueWrapper>)
                ))
            }
            SocketType::Color => {
                let label = _gs.with_graph(|g| {
                    g.world.get::<nodegraph_core::graph::port::PortLabel>(port_id)
                        .map(|l| l.0.clone())
                        .unwrap_or_default()
                });
                let default = default_color(type_id, &label);
                let mutable = params.get_color(port_id, default);
                Some(color_picker(mutable))
            }
            _ => None,
        }
    })
}

/// Minimal inline color picker: a small swatch that wraps an `<input type="color">`.
fn color_picker(value: Mutable<[u8; 4]>) -> dominator::Dom {
    html!("div", {
        .attr("data-port-widget", "")
        .style("position", "relative")
        .style("width", "100%")
        .style("height", "16px")
        .style("pointer-events", "auto")

        // Visible swatch
        .child(html!("div", {
            .style("width", "100%")
            .style("height", "100%")
            .style("border-radius", "2px")
            .style("border", "1px solid #555")
            .style("cursor", "pointer")
            .style_signal("background", value.signal().map(|c| {
                format!("rgb({},{},{})", c[0], c[1], c[2])
            }))
        }))

        // Hidden native color input overlaid
        .child(html!("input" => web_sys::HtmlInputElement, {
            .attr("type", "color")
            .style("position", "absolute")
            .style("top", "0")
            .style("left", "0")
            .style("width", "100%")
            .style("height", "100%")
            .style("opacity", "0")
            .style("cursor", "pointer")
            .attr_signal("value", value.signal().map(rgba_to_hex))
            .event(clone!(value => move |e: events::Input| {
                let target: web_sys::HtmlInputElement = e.target().unwrap().unchecked_into();
                let hex = target.value();
                value.set(hex_to_rgba(&hex));
            }))
        }))
    })
}
