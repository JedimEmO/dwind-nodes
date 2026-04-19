use std::rc::Rc;

use nodegraph_core::types::socket_type::SocketType;
use nodegraph_core::{EntityId, PortDirection};
use nodegraph_render::GraphSignals;
use nodegraph_runtime::prelude::ParamStore;
use nodegraph_widgets::bool_input::{bool_input, BoolInputProps, BoolValueWrapper};
use nodegraph_widgets::color_input::{color_input, ColorInputProps, ColorValueWrapper};
use nodegraph_widgets::float_input::{float_input, FloatInputProps, FloatValueWrapper};
use nodegraph_widgets::int_input::{int_input, IntInputProps, IntValueWrapper};
use nodegraph_widgets::string_input::{string_input, StringInputProps, StringValueWrapper};

/// Default float values per node type + port label.
pub fn default_float(type_id: &str, label: &str) -> f64 {
    match (type_id, label) {
        ("const_float", "Value") => 1.0,
        ("noise", "Scale") => 5.0,
        ("mix", "Factor") => 0.5,
        ("brightness_contrast", "Brightness") => 0.0,
        ("brightness_contrast", "Contrast") => 0.0,
        ("threshold", "Level") => 0.5,
        _ => 0.0,
    }
}

/// Default integer values per node type + port label.
pub fn default_int(type_id: &str, label: &str) -> i64 {
    match (type_id, label) {
        ("const_int", "Value") => 1,
        ("checker", "Size") => 4,
        ("noise", "Seed") => 1,
        ("brick", "Rows") => 4,
        _ => 0,
    }
}

/// Default boolean values per node type + port label.
pub fn default_bool(type_id: &str, label: &str) -> bool {
    matches!((type_id, label), ("brick", "Stagger"))
}

/// Default string values per node type + port label.
pub fn default_string(_type_id: &str, _label: &str) -> String {
    String::new()
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

/// Node types whose output port should always show an editable widget,
/// regardless of connection state.
pub fn is_const_output_type(type_id: &str) -> bool {
    matches!(
        type_id,
        "solid_color" | "const_float" | "const_int" | "const_bool" | "const_string"
    )
}

fn port_label(gs: &Rc<GraphSignals>, port_id: EntityId) -> String {
    gs.with_graph(|g| {
        g.world
            .get::<nodegraph_core::graph::port::PortLabel>(port_id)
            .map(|l| l.0.clone())
            .unwrap_or_default()
    })
}

fn widget_for(
    params: &ParamStore,
    port_id: EntityId,
    socket_type: SocketType,
    type_id: &str,
    label: &str,
) -> Option<dominator::Dom> {
    match socket_type {
        SocketType::Float => {
            let default = default_float(type_id, label);
            let mutable = params.get::<f64>(port_id, default);
            Some(float_input(
                FloatInputProps::new().value(Box::new(mutable) as Box<dyn FloatValueWrapper>),
            ))
        }
        SocketType::Int => {
            let default = default_int(type_id, label);
            let mutable = params.get::<i64>(port_id, default);
            Some(int_input(
                IntInputProps::new().value(Box::new(mutable) as Box<dyn IntValueWrapper>),
            ))
        }
        SocketType::Bool => {
            let default = default_bool(type_id, label);
            let mutable = params.get::<bool>(port_id, default);
            Some(bool_input(
                BoolInputProps::new().value(Box::new(mutable) as Box<dyn BoolValueWrapper>),
            ))
        }
        SocketType::String => {
            let default = default_string(type_id, label);
            let mutable = params.get::<String>(port_id, default);
            Some(string_input(
                StringInputProps::new().value(Box::new(mutable) as Box<dyn StringValueWrapper>),
            ))
        }
        SocketType::Color => {
            let default = default_color(type_id, label);
            let mutable = params.get::<[u8; 4]>(port_id, default);
            Some(color_input(
                ColorInputProps::new().value(Box::new(mutable) as Box<dyn ColorValueWrapper>),
            ))
        }
        _ => None,
    }
}

/// Build the `port_widget` callback for the texture generator.
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
        move |_node_id, port_id, socket_type, port_dir, type_id, is_connected, gs| {
            let show_on_output = port_dir == PortDirection::Output && is_const_output_type(type_id);
            let show_on_input = port_dir == PortDirection::Input && !is_connected;

            if !show_on_output && !show_on_input {
                return None;
            }

            let label = port_label(gs, port_id);
            widget_for(&params, port_id, socket_type, type_id, &label)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn default_float_known() {
        assert_eq!(default_float("noise", "Scale"), 5.0);
        assert_eq!(default_float("threshold", "Level"), 0.5);
    }

    #[wasm_bindgen_test]
    fn default_int_known() {
        assert_eq!(default_int("checker", "Size"), 4);
        assert_eq!(default_int("noise", "Seed"), 1);
        assert_eq!(default_int("brick", "Rows"), 4);
    }

    #[wasm_bindgen_test]
    fn default_bool_known() {
        assert!(default_bool("brick", "Stagger"));
        assert!(!default_bool("anything", "else"));
    }

    #[wasm_bindgen_test]
    fn default_color_known() {
        assert_eq!(default_color("solid_color", "Color"), [139, 105, 20, 255]);
    }

    #[wasm_bindgen_test]
    fn default_float_unknown_zero() {
        assert_eq!(default_float("nonexistent", "Whatever"), 0.0);
    }

    #[wasm_bindgen_test]
    fn default_color_unknown_gray() {
        assert_eq!(
            default_color("nonexistent", "Whatever"),
            [200, 200, 200, 255]
        );
    }

    #[wasm_bindgen_test]
    fn is_const_output_matches() {
        assert!(is_const_output_type("const_float"));
        assert!(is_const_output_type("solid_color"));
        assert!(!is_const_output_type("noise"));
    }
}
