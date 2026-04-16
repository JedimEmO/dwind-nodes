use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dominator::{clone, events, html};
use futures_signals::signal::{Mutable, SignalExt};
use wasm_bindgen::JsCast;

use nodegraph_core::{EntityId, PortDirection, SocketType};
use nodegraph_render::GraphSignals;
use nodegraph_widgets::float_input::{float_input, FloatInputProps};

pub struct ParamStore {
    floats: RefCell<HashMap<EntityId, Mutable<f64>>>,
    colors: RefCell<HashMap<EntityId, Mutable<[u8; 4]>>>,
}

impl ParamStore {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            floats: RefCell::new(HashMap::new()),
            colors: RefCell::new(HashMap::new()),
        })
    }

    pub fn get_float(&self, port_id: EntityId, default: f64) -> Mutable<f64> {
        self.floats
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(default))
            .clone()
    }

    pub fn get_color(&self, port_id: EntityId, default: [u8; 4]) -> Mutable<[u8; 4]> {
        self.colors
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(default))
            .clone()
    }

    /// Migrate param values from old port IDs to new port IDs.
    /// Used after group/ungroup which recreates ports with fresh EntityIds.
    pub fn migrate_ports(&self, old_to_new: &HashMap<EntityId, EntityId>) {
        let mut floats = self.floats.borrow_mut();
        for (old_id, new_id) in old_to_new {
            if let Some(m) = floats.get(old_id).cloned() {
                floats.insert(*new_id, m);
            }
        }
        drop(floats);
        let mut colors = self.colors.borrow_mut();
        for (old_id, new_id) in old_to_new {
            if let Some(m) = colors.get(old_id).cloned() {
                colors.insert(*new_id, m);
            }
        }
    }

    /// Snapshot all current param values into plain HashMaps (for group node evaluation).
    pub fn snapshot(&self) -> crate::eval::ParamSnapshot {
        let floats = self.floats.borrow();
        let colors = self.colors.borrow();
        crate::eval::ParamSnapshot {
            floats: floats.iter().map(|(&k, v)| (k, v.get())).collect(),
            colors: colors.iter().map(|(&k, v)| (k, v.get())).collect(),
        }
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
        ("solid_color", "Color") => [139, 105, 20, 255], // brown
        ("checker", "Color A") => [180, 180, 180, 255],  // light gray
        ("checker", "Color B") => [80, 80, 80, 255],     // dark gray
        ("gradient", "Color A") => [20, 20, 60, 255],    // dark blue
        ("gradient", "Color B") => [200, 180, 140, 255], // sand
        ("brick", "Mortar") => [120, 120, 120, 255],     // gray mortar
        ("brick", "Brick") => [160, 80, 60, 255],        // red brick
        ("colorize", "Tint") => [139, 105, 20, 255],     // brown
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
#[allow(clippy::type_complexity)]
pub fn make_port_widget(
    params: &Rc<ParamStore>,
) -> Rc<
    dyn Fn(
        EntityId,
        EntityId,
        SocketType,
        PortDirection,
        &str,
        bool,
        &Rc<GraphSignals>,
    ) -> Option<dominator::Dom>,
> {
    let params = params.clone();
    Rc::new(
        move |_node_id, port_id, socket_type, port_dir, type_id, is_connected, _gs| {
            // Solid Color: always show color picker on its output port
            if type_id == "solid_color" && port_dir == PortDirection::Output {
                let label = _gs.with_graph(|g| {
                    g.world
                        .get::<nodegraph_core::graph::port::PortLabel>(port_id)
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
                        g.world
                            .get::<nodegraph_core::graph::port::PortLabel>(port_id)
                            .map(|l| l.0.clone())
                            .unwrap_or_default()
                    });
                    let default = default_float(type_id, &label);
                    let mutable = params.get_float(port_id, default);
                    Some(float_input(FloatInputProps::new().value(
                        Box::new(mutable) as Box<dyn nodegraph_widgets::FloatValueWrapper>
                    )))
                }
                SocketType::Color => {
                    let label = _gs.with_graph(|g| {
                        g.world
                            .get::<nodegraph_core::graph::port::PortLabel>(port_id)
                            .map(|l| l.0.clone())
                            .unwrap_or_default()
                    });
                    let default = default_color(type_id, &label);
                    let mutable = params.get_color(port_id, default);
                    Some(color_picker(mutable))
                }
                _ => None,
            }
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn default_float_known() {
        assert_eq!(default_float("checker", "Size"), 4.0);
        assert_eq!(default_float("noise", "Scale"), 5.0);
        assert_eq!(default_float("noise", "Seed"), 1.0);
        assert_eq!(default_float("threshold", "Level"), 0.5);
    }

    #[wasm_bindgen_test]
    fn default_float_unknown_zero() {
        assert_eq!(default_float("nonexistent", "Whatever"), 0.0);
    }

    #[wasm_bindgen_test]
    fn default_color_known() {
        assert_eq!(default_color("solid_color", "Color"), [139, 105, 20, 255]);
    }

    #[wasm_bindgen_test]
    fn default_color_unknown_gray() {
        assert_eq!(
            default_color("nonexistent", "Whatever"),
            [200, 200, 200, 255]
        );
    }

    #[wasm_bindgen_test]
    fn get_float_default() {
        use nodegraph_core::store::World;
        let mut world = World::new();
        let id = world.spawn();

        let store = ParamStore::new();
        let m = store.get_float(id, 3.5);
        assert_eq!(m.get(), 3.5);
    }

    #[wasm_bindgen_test]
    fn get_float_same_mutable() {
        use nodegraph_core::store::World;
        let mut world = World::new();
        let id = world.spawn();

        let store = ParamStore::new();
        let m1 = store.get_float(id, 1.0);
        let m2 = store.get_float(id, 999.0); // default ignored on second call
        m1.set(42.0);
        assert_eq!(m2.get(), 42.0);
    }

    #[wasm_bindgen_test]
    fn get_color_default() {
        use nodegraph_core::store::World;
        let mut world = World::new();
        let id = world.spawn();

        let store = ParamStore::new();
        let m = store.get_color(id, [10, 20, 30, 255]);
        assert_eq!(m.get(), [10, 20, 30, 255]);
    }

    #[wasm_bindgen_test]
    fn hex_rgba_roundtrip() {
        assert_eq!(hex_to_rgba("#8b6914"), [139, 105, 20, 255]);
        let hex = rgba_to_hex([139, 105, 20, 255]);
        assert_eq!(hex_to_rgba(&hex), [139, 105, 20, 255]);
    }
}
