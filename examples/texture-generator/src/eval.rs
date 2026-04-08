use std::collections::HashMap;
use std::rc::Rc;

use nodegraph_core::EntityId;
use nodegraph_core::graph::node::NodeTypeId;
use nodegraph_core::graph::port::{PortDirection, PortLabel, PortSocketType};
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::graph::group::SubgraphRoot;
use nodegraph_core::graph::{GraphEditor, GroupIOKind, NodeGraph};
use nodegraph_core::types::socket_type::SocketType;

use crate::texture::{TextureBuffer, TEX_SIZE};

/// Snapshot of all param values — plain data, no RefCells or signals.
/// Used by group node evaluation and testing.
pub struct ParamSnapshot {
    pub floats: HashMap<EntityId, f64>,
    pub colors: HashMap<EntityId, [u8; 4]>,
}

pub struct EvalResult {
    /// Textures keyed by **output port** EntityId.
    pub textures: HashMap<EntityId, Rc<TextureBuffer>>,
}

/// Evaluate the entire graph (including subgraphs) and return textures keyed by output port.
/// Used by group node computation in ReactiveEval.
pub fn evaluate(editor: &GraphEditor, snap: &ParamSnapshot) -> EvalResult {
    let mut textures: HashMap<EntityId, Rc<TextureBuffer>> = HashMap::new();
    let mut colors: HashMap<EntityId, [u8; 4]> = HashMap::new();

    let root_id = editor.root_graph_id();
    let graph = editor.graph(root_id).expect("root graph");

    eval_graph(graph, editor, snap, &mut textures, &mut colors);

    EvalResult { textures }
}

fn eval_graph(
    graph: &NodeGraph,
    editor: &GraphEditor,
    snap: &ParamSnapshot,
    textures: &mut HashMap<EntityId, Rc<TextureBuffer>>,
    colors: &mut HashMap<EntityId, [u8; 4]>,
) {
    let eval_order = graph.eval_order();

    for node_id in eval_order {
        if let Some(sub_root) = graph.world.get::<SubgraphRoot>(node_id) {
            eval_group_node(node_id, sub_root.0, graph, editor, snap, textures, colors);
            continue;
        }

        if graph.world.get::<GroupIOKind>(node_id).is_some() {
            continue;
        }

        let type_id = graph.world.get::<NodeTypeId>(node_id)
            .map(|t| t.0.clone())
            .unwrap_or_default();

        eval_node(node_id, &type_id, graph, snap, textures, colors);
    }
}

fn eval_group_node(
    group_node_id: EntityId,
    subgraph_id: EntityId,
    parent_graph: &NodeGraph,
    editor: &GraphEditor,
    snap: &ParamSnapshot,
    textures: &mut HashMap<EntityId, Rc<TextureBuffer>>,
    colors: &mut HashMap<EntityId, [u8; 4]>,
) {
    let subgraph = match editor.graph(subgraph_id) {
        Some(g) => g,
        None => return,
    };

    // Map group node input port values → subgraph Group Input IO node output ports.
    for &gport in parent_graph.node_ports(group_node_id) {
        let dir = parent_graph.world.get::<PortDirection>(gport).copied().unwrap_or(PortDirection::Output);
        if dir != PortDirection::Input { continue; }

        let upstream_color = find_upstream(gport, parent_graph, colors);
        let upstream_tex = find_upstream(gport, parent_graph, textures);

        // Reverse-lookup: group_port → subgraph IO port
        for (&(sid, io_port), &mapped_gport) in &editor.io_port_mapping {
            if sid != subgraph_id || mapped_gport != gport { continue; }

            if let Some(c) = upstream_color {
                colors.insert(io_port, c);
            }
            if let Some(ref t) = upstream_tex {
                textures.insert(io_port, t.clone());
            }
        }
    }

    // Evaluate the subgraph
    let sub_order = subgraph.eval_order();
    for sub_node_id in sub_order {
        if subgraph.world.get::<GroupIOKind>(sub_node_id).is_some() {
            continue;
        }
        if let Some(sub_root) = subgraph.world.get::<SubgraphRoot>(sub_node_id) {
            eval_group_node(sub_node_id, sub_root.0, subgraph, editor, snap, textures, colors);
            continue;
        }
        let type_id = subgraph.world.get::<NodeTypeId>(sub_node_id)
            .map(|t| t.0.clone())
            .unwrap_or_default();
        eval_node(sub_node_id, &type_id, subgraph, snap, textures, colors);
    }

    // Map subgraph Group Output IO node inputs → group node output ports.
    for &gport in parent_graph.node_ports(group_node_id) {
        let dir = parent_graph.world.get::<PortDirection>(gport).copied().unwrap_or(PortDirection::Input);
        if dir != PortDirection::Output { continue; }

        for (&(sid, io_port), &mapped_gport) in &editor.io_port_mapping {
            if sid != subgraph_id || mapped_gport != gport { continue; }

            if let Some(t) = find_upstream(io_port, subgraph, textures) {
                textures.insert(gport, t);
            }
            if let Some(c) = find_upstream(io_port, subgraph, colors) {
                colors.insert(gport, c);
            }
        }
    }
}

/// Trace a connection from an input port back to its source output port,
/// then look up the source port's value in the given map.
fn find_upstream<T: Clone>(
    port_id: EntityId, graph: &NodeGraph, values: &HashMap<EntityId, T>,
) -> Option<T> {
    for &conn_id in graph.port_connections(port_id) {
        let ep = graph.world.get::<ConnectionEndpoints>(conn_id)?;
        if ep.target_port != port_id { continue; }
        // Look up by source PORT, not source node
        if let Some(v) = values.get(&ep.source_port) {
            return Some(v.clone());
        }
    }
    None
}

/// Evaluate a single non-group node. Stores results keyed by output port ID.
fn eval_node(
    node_id: EntityId,
    type_id: &str,
    graph: &NodeGraph,
    snap: &ParamSnapshot,
    textures: &mut HashMap<EntityId, Rc<TextureBuffer>>,
    colors: &mut HashMap<EntityId, [u8; 4]>,
) {
    let get_input_texture = |label: &str| -> Option<Rc<TextureBuffer>> {
        for &pid in graph.node_ports(node_id) {
            if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }
            let pl = graph.world.get::<PortLabel>(pid).map(|l| l.0.as_str()).unwrap_or("");
            if pl != label { continue; }
            return find_upstream(pid, graph, textures);
        }
        None
    };

    let get_float = |label: &str| -> f64 {
        for &pid in graph.node_ports(node_id) {
            if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }
            let pl = graph.world.get::<PortLabel>(pid).map(|l| l.0.as_str()).unwrap_or("");
            if pl != label { continue; }
            let default = crate::params::default_float(type_id, label);
            return snap.floats.get(&pid).copied().unwrap_or(default);
        }
        0.0
    };

    let get_color = |label: &str| -> [u8; 4] {
        for &pid in graph.node_ports(node_id) {
            if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }
            let pl = graph.world.get::<PortLabel>(pid).map(|l| l.0.as_str()).unwrap_or("");
            if pl != label { continue; }
            // Check connected upstream color (by source port)
            if let Some(c) = find_upstream(pid, graph, colors) {
                return c;
            }
            let default = crate::params::default_color(type_id, label);
            return snap.colors.get(&pid).copied().unwrap_or(default);
        }
        [200, 200, 200, 255]
    };

    // Compute the result and store keyed by each output port
    let store_texture = |tex: TextureBuffer, textures: &mut HashMap<EntityId, Rc<TextureBuffer>>| {
        let tex = Rc::new(tex);
        for &pid in graph.node_ports(node_id) {
            if graph.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Output)
                && graph.world.get::<PortSocketType>(pid).map(|s| s.0) == Some(SocketType::Image)
            {
                textures.insert(pid, tex.clone());
            }
        }
    };

    match type_id {
        "solid_color" => {
            for &pid in graph.node_ports(node_id) {
                if graph.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Output) {
                    let default = crate::params::default_color("solid_color", "Color");
                    let color = snap.colors.get(&pid).copied().unwrap_or(default);
                    colors.insert(pid, color);
                }
            }
        }
        "checker" => store_texture(eval_checker(get_color("Color A"), get_color("Color B"), get_float("Size")), textures),
        "noise" => store_texture(eval_noise(get_float("Scale"), get_float("Seed")), textures),
        "gradient" => store_texture(eval_gradient(get_color("Color A"), get_color("Color B")), textures),
        "brick" => store_texture(eval_brick(get_color("Mortar"), get_color("Brick"), get_float("Rows")), textures),
        "mix" => store_texture(eval_mix(get_input_texture("A"), get_input_texture("B"), get_float("Factor")), textures),
        "brightness_contrast" => store_texture(eval_brightness_contrast(get_input_texture("Texture"), get_float("Brightness"), get_float("Contrast")), textures),
        "threshold" => store_texture(eval_threshold(get_input_texture("Texture"), get_float("Level")), textures),
        "invert" => store_texture(eval_invert(get_input_texture("Texture")), textures),
        "colorize" => store_texture(eval_colorize(get_input_texture("Texture"), get_color("Tint")), textures),
        "preview" | "tiled_preview" | "iso_preview" => {
            for &pid in graph.node_ports(node_id) {
                if graph.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Input) {
                    if let Some(t) = find_upstream(pid, graph, textures) {
                        textures.insert(pid, t);
                    } else {
                        textures.insert(pid, Rc::new(TextureBuffer::new()));
                    }
                }
            }
        }
        _ => {}
    }
}

// ============================================================
// Texture generation functions
// ============================================================

pub(crate) fn eval_checker(color_a: [u8; 4], color_b: [u8; 4], size: f64) -> TextureBuffer {
    let mut tex = TextureBuffer::new();
    let cell = (size as usize).max(1);
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let checker = ((x / cell) + (y / cell)).is_multiple_of(2);
            tex.set(x, y, if checker { color_a } else { color_b });
        }
    }
    tex
}

pub(crate) fn eval_noise(scale: f64, seed: f64) -> TextureBuffer {
    let mut tex = TextureBuffer::new();
    let seed_bits = (seed * 12345.6789) as u32;
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let v = value_noise(x as f64 * scale / TEX_SIZE as f64, y as f64 * scale / TEX_SIZE as f64, seed_bits);
            let c = (v * 255.0) as u8;
            tex.set(x, y, [c, c, c, 255]);
        }
    }
    tex
}

pub(crate) fn value_noise(x: f64, y: f64, seed: u32) -> f64 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let v00 = hash_f(ix, iy, seed);
    let v10 = hash_f(ix + 1, iy, seed);
    let v01 = hash_f(ix, iy + 1, seed);
    let v11 = hash_f(ix + 1, iy + 1, seed);
    let a = v00 + (v10 - v00) * fx;
    let b = v01 + (v11 - v01) * fx;
    a + (b - a) * fy
}

pub(crate) fn hash_f(x: i32, y: i32, seed: u32) -> f64 {
    let mut h = seed;
    h ^= x as u32;
    h = h.wrapping_mul(0x45d9f3b);
    h ^= y as u32;
    h = h.wrapping_mul(0x45d9f3b);
    h ^= h >> 16;
    (h & 0xFFFF) as f64 / 65535.0
}

pub(crate) fn eval_gradient(color_a: [u8; 4], color_b: [u8; 4]) -> TextureBuffer {
    let mut tex = TextureBuffer::new();
    for y in 0..TEX_SIZE {
        let t = y as f64 / (TEX_SIZE - 1) as f64;
        let c = lerp_color(color_a, color_b, t);
        for x in 0..TEX_SIZE {
            tex.set(x, y, c);
        }
    }
    tex
}

pub(crate) fn eval_brick(mortar: [u8; 4], brick: [u8; 4], rows: f64) -> TextureBuffer {
    let mut tex = TextureBuffer::new();
    let row_count = (rows as usize).max(1);
    let row_h = TEX_SIZE / row_count;
    let mortar_w = 1;
    for y in 0..TEX_SIZE {
        let row = y / row_h.max(1);
        let is_mortar_row = row_h > 1 && (y % row_h.max(1)) < mortar_w;
        let offset = if row % 2 == 1 { TEX_SIZE / 2 } else { 0 };
        for x in 0..TEX_SIZE {
            let bx = (x + offset) % TEX_SIZE;
            let is_mortar_col = bx < mortar_w || bx >= TEX_SIZE - mortar_w;
            if is_mortar_row || (row_h > 2 && is_mortar_col) {
                tex.set(x, y, mortar);
            } else {
                tex.set(x, y, brick);
            }
        }
    }
    tex
}

pub(crate) fn eval_mix(a: Option<Rc<TextureBuffer>>, b: Option<Rc<TextureBuffer>>, factor: f64) -> TextureBuffer {
    let black = Rc::new(TextureBuffer::new());
    let a = a.unwrap_or_else(|| black.clone());
    let b = b.unwrap_or_else(|| black.clone());
    let f = factor.clamp(0.0, 1.0);
    let mut tex = TextureBuffer::new();
    for i in 0..TEX_SIZE * TEX_SIZE {
        tex.data[i] = lerp_color(a.data[i], b.data[i], f);
    }
    tex
}

pub(crate) fn eval_brightness_contrast(input: Option<Rc<TextureBuffer>>, brightness: f64, contrast: f64) -> TextureBuffer {
    let input = input.unwrap_or_else(|| Rc::new(TextureBuffer::new()));
    let mut tex = TextureBuffer::new();
    let b = brightness * 255.0;
    let c = contrast + 1.0;
    for i in 0..TEX_SIZE * TEX_SIZE {
        let [r, g, bl, a] = input.data[i];
        tex.data[i] = [
            ((((r as f64 - 128.0) * c) + 128.0 + b) as i32).clamp(0, 255) as u8,
            ((((g as f64 - 128.0) * c) + 128.0 + b) as i32).clamp(0, 255) as u8,
            ((((bl as f64 - 128.0) * c) + 128.0 + b) as i32).clamp(0, 255) as u8,
            a,
        ];
    }
    tex
}

pub(crate) fn eval_threshold(input: Option<Rc<TextureBuffer>>, level: f64) -> TextureBuffer {
    let input = input.unwrap_or_else(|| Rc::new(TextureBuffer::new()));
    let threshold = (level * 255.0) as u16;
    let mut tex = TextureBuffer::new();
    for i in 0..TEX_SIZE * TEX_SIZE {
        let [r, g, b, a] = input.data[i];
        let lum = (r as u16 + g as u16 + b as u16) / 3;
        let v = if lum >= threshold { 255u8 } else { 0u8 };
        tex.data[i] = [v, v, v, a];
    }
    tex
}

pub(crate) fn eval_invert(input: Option<Rc<TextureBuffer>>) -> TextureBuffer {
    let input = input.unwrap_or_else(|| Rc::new(TextureBuffer::new()));
    let mut tex = TextureBuffer::new();
    for i in 0..TEX_SIZE * TEX_SIZE {
        let [r, g, b, a] = input.data[i];
        tex.data[i] = [255 - r, 255 - g, 255 - b, a];
    }
    tex
}

pub(crate) fn eval_colorize(input: Option<Rc<TextureBuffer>>, tint: [u8; 4]) -> TextureBuffer {
    let input = input.unwrap_or_else(|| Rc::new(TextureBuffer::new()));
    let mut tex = TextureBuffer::new();
    for i in 0..TEX_SIZE * TEX_SIZE {
        let [r, g, b, a] = input.data[i];
        tex.data[i] = [
            ((r as u16 * tint[0] as u16) / 255) as u8,
            ((g as u16 * tint[1] as u16) / 255) as u8,
            ((b as u16 * tint[2] as u16) / 255) as u8,
            a,
        ];
    }
    tex
}

pub(crate) fn lerp_color(a: [u8; 4], b: [u8; 4], t: f64) -> [u8; 4] {
    [
        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t) as u8,
        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t) as u8,
        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t) as u8,
        (a[3] as f64 + (b[3] as f64 - a[3] as f64) * t) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use nodegraph_core::graph::GraphEditor;
    use nodegraph_core::graph::node::NodeTypeId;
    use nodegraph_core::PortDirection;
    use nodegraph_core::types::socket_type::SocketType;
    use crate::texture::{TextureBuffer, TEX_SIZE};


    // ============================================================
    // Helper function tests
    // ============================================================

    #[wasm_bindgen_test]
    fn hash_f_deterministic() {
        let a = hash_f(3, 7, 42);
        let b = hash_f(3, 7, 42);
        assert_eq!(a, b, "same inputs must produce same output");

        let c = hash_f(3, 7, 99);
        assert_ne!(a, c, "different seed must produce different output");
    }

    #[wasm_bindgen_test]
    fn hash_f_range() {
        for x in -50..50 {
            for y in -10..10 {
                let v = hash_f(x, y, 1234);
                assert!((0.0..=1.0).contains(&v), "hash_f({x},{y},1234) = {v} out of [0,1]");
            }
        }
    }

    #[wasm_bindgen_test]
    fn value_noise_deterministic() {
        let a = value_noise(1.5, 2.3, 42);
        let b = value_noise(1.5, 2.3, 42);
        assert_eq!(a, b, "same inputs must produce same output");
    }

    #[wasm_bindgen_test]
    fn value_noise_range() {
        for ix in 0..20 {
            for iy in 0..20 {
                let x = ix as f64 * 0.37;
                let y = iy as f64 * 0.37;
                let v = value_noise(x, y, 77);
                assert!((0.0..=1.0).contains(&v), "value_noise({x},{y},77) = {v} out of [0,1]");
            }
        }
    }

    #[wasm_bindgen_test]
    fn lerp_color_endpoints() {
        let a = [10, 20, 30, 40];
        let b = [200, 150, 100, 250];
        assert_eq!(lerp_color(a, b, 0.0), a, "t=0 must return a");
        assert_eq!(lerp_color(a, b, 1.0), b, "t=1 must return b");
    }

    // ============================================================
    // Generator tests
    // ============================================================

    #[wasm_bindgen_test]
    fn checker_alternating() {
        let white = [255, 255, 255, 255];
        let black = [0, 0, 0, 255];
        let tex = eval_checker(white, black, 4.0);
        // (0,0) -> cell (0+0)=0, even -> color_a
        assert_eq!(tex.data[0], white, "(0,0) should be color_a");
        // (4,0) -> cell (1+0)=1, odd -> color_b
        assert_eq!(tex.data[4], black, "(4,0) should be color_b");
    }

    #[wasm_bindgen_test]
    fn checker_size_zero_clamps() {
        // size=0 should clamp to cell=1 and not panic
        let tex = eval_checker([255, 0, 0, 255], [0, 255, 0, 255], 0.0);
        assert_eq!(tex.data.len(), TEX_SIZE * TEX_SIZE);
    }

    #[wasm_bindgen_test]
    fn noise_deterministic() {
        let a = eval_noise(5.0, 1.0);
        let b = eval_noise(5.0, 1.0);
        assert_eq!(a.data, b.data, "same params must produce identical buffers");
    }

    #[wasm_bindgen_test]
    fn noise_different_seeds() {
        let a = eval_noise(5.0, 1.0);
        let b = eval_noise(5.0, 2.0);
        assert_ne!(a.data, b.data, "different seeds must produce different output");
    }

    #[wasm_bindgen_test]
    fn gradient_top_bottom() {
        let top = [255, 0, 0, 255];
        let bot = [0, 0, 255, 255];
        let tex = eval_gradient(top, bot);
        // Row 0 should be color_a
        assert_eq!(tex.data[0], top, "row 0 should be color_a");
        // Row 15 should be color_b
        assert_eq!(tex.data[15 * TEX_SIZE], bot, "row 15 should be color_b");
    }

    #[wasm_bindgen_test]
    fn brick_has_mortar() {
        let mortar = [200, 200, 200, 255];
        let brick = [160, 80, 60, 255];
        let tex = eval_brick(mortar, brick, 4.0);
        let mortar_count = tex.data.iter().filter(|&&px| px == mortar).count();
        assert!(mortar_count > 0, "brick texture must contain mortar pixels");
        let brick_count = tex.data.iter().filter(|&&px| px == brick).count();
        assert!(brick_count > 0, "brick texture must contain brick pixels");
    }

    // ============================================================
    // Filter tests
    // ============================================================

    fn make_solid(color: [u8; 4]) -> TextureBuffer {
        let mut tex = TextureBuffer::new();
        for i in 0..TEX_SIZE * TEX_SIZE {
            tex.data[i] = color;
        }
        tex
    }

    #[wasm_bindgen_test]
    fn mix_factor_zero() {
        let a = Rc::new(make_solid([255, 0, 0, 255]));
        let b = Rc::new(make_solid([0, 0, 255, 255]));
        let result = eval_mix(Some(a.clone()), Some(b), 0.0);
        assert_eq!(result.data, a.data, "factor=0 must return texture A");
    }

    #[wasm_bindgen_test]
    fn mix_factor_one() {
        let a = Rc::new(make_solid([255, 0, 0, 255]));
        let b = Rc::new(make_solid([0, 0, 255, 255]));
        let result = eval_mix(Some(a), Some(b.clone()), 1.0);
        assert_eq!(result.data, b.data, "factor=1 must return texture B");
    }

    #[wasm_bindgen_test]
    fn mix_none_black() {
        let result = eval_mix(None, None, 0.5);
        let black = TextureBuffer::new();
        assert_eq!(result.data, black.data, "both None must produce all black");
    }

    #[wasm_bindgen_test]
    fn brightness_positive() {
        let gray = make_solid([100, 100, 100, 255]);
        let result = eval_brightness_contrast(Some(Rc::new(gray)), 0.5, 0.0);
        // brightness=0.5 adds 127.5 to each channel
        for px in &result.data {
            assert!(px[0] > 100, "brightness should increase channel values");
        }
    }

    #[wasm_bindgen_test]
    fn contrast_increases() {
        // Use a texture with a mid-dark and mid-bright pixel
        let mut tex = TextureBuffer::new();
        tex.data[0] = [80, 80, 80, 255];   // below midpoint
        tex.data[1] = [200, 200, 200, 255]; // above midpoint
        let result = eval_brightness_contrast(Some(Rc::new(tex)), 0.0, 1.0);
        // contrast=1.0 -> c=2.0, so dark gets darker, bright gets brighter
        assert!(result.data[0][0] < 80, "dark pixel should get darker with contrast");
        assert!(result.data[1][0] > 200, "bright pixel should get brighter with contrast");
    }

    #[wasm_bindgen_test]
    fn threshold_zero_white() {
        let gray = make_solid([128, 128, 128, 255]);
        // level near 0 -> threshold = ~0, so luminance >= 0 -> white
        let result = eval_threshold(Some(Rc::new(gray)), 0.001);
        for px in &result.data {
            assert_eq!(px[0], 255, "near-zero threshold should produce white");
        }
    }

    #[wasm_bindgen_test]
    fn threshold_one_black() {
        let gray = make_solid([128, 128, 128, 255]);
        let result = eval_threshold(Some(Rc::new(gray)), 1.0);
        // level=1.0 -> threshold=255, luminance=128 < 255 -> black
        for px in &result.data {
            assert_eq!(px[0], 0, "level=1.0 should produce all black");
        }
    }

    #[wasm_bindgen_test]
    fn invert_roundtrip() {
        let original = eval_noise(5.0, 1.0);
        let inverted = eval_invert(Some(Rc::new(original.clone())));
        let double_inverted = eval_invert(Some(Rc::new(inverted)));
        assert_eq!(original.data, double_inverted.data, "double invert must equal identity");
    }

    #[wasm_bindgen_test]
    fn colorize_white_tint() {
        let white = make_solid([255, 255, 255, 255]);
        let tint = [100, 150, 200, 255];
        let result = eval_colorize(Some(Rc::new(white)), tint);
        // white (255) * tint / 255 = tint
        for px in &result.data {
            assert_eq!(px[0], 100);
            assert_eq!(px[1], 150);
            assert_eq!(px[2], 200);
        }
    }

    #[wasm_bindgen_test]
    fn colorize_black_stays() {
        let black = TextureBuffer::new(); // all [0,0,0,255]
        let tint = [100, 150, 200, 255];
        let result = eval_colorize(Some(Rc::new(black)), tint);
        for px in &result.data {
            assert_eq!(px[0], 0, "black stays black after colorize");
            assert_eq!(px[1], 0);
            assert_eq!(px[2], 0);
        }
    }

    // ============================================================
    // Pipeline tests (full evaluate() with GraphEditor)
    // ============================================================

    /// Helper: create a node in the root graph with a type ID and given ports.
    /// Returns (node_id, vec_of_port_ids).
    fn make_node(
        editor: &mut GraphEditor,
        title: &str,
        type_id: &str,
        ports: &[(PortDirection, SocketType, &str)],
    ) -> (EntityId, Vec<EntityId>) {
        let graph = editor.current_graph_mut();
        let node_id = graph.add_node(title, (0.0, 0.0));
        graph.world.insert(node_id, NodeTypeId(type_id.to_string()));
        let mut port_ids = Vec::new();
        for &(dir, st, label) in ports {
            let pid = graph.add_port(node_id, dir, st, label);
            port_ids.push(pid);
        }
        (node_id, port_ids)
    }

    #[wasm_bindgen_test]
    fn eval_single_checker() {
        let mut editor = GraphEditor::new();
        let (_node_id, ports) = make_node(&mut editor, "Checker", "checker", &[
            (PortDirection::Input, SocketType::Color, "Color A"),
            (PortDirection::Input, SocketType::Color, "Color B"),
            (PortDirection::Input, SocketType::Float, "Size"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);
        let output_port = ports[3];

        let snap = ParamSnapshot {
            floats: HashMap::new(),
            colors: HashMap::new(),
        };
        let result = evaluate(&editor, &snap);
        assert!(result.textures.contains_key(&output_port), "output port must have a texture");
        assert_eq!(result.textures[&output_port].data.len(), TEX_SIZE * TEX_SIZE);
    }

    #[wasm_bindgen_test]
    fn eval_chain_noise_invert() {
        let mut editor = GraphEditor::new();
        let (_noise_id, noise_ports) = make_node(&mut editor, "Noise", "noise", &[
            (PortDirection::Input, SocketType::Float, "Scale"),
            (PortDirection::Input, SocketType::Float, "Seed"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);
        let (_invert_id, invert_ports) = make_node(&mut editor, "Invert", "invert", &[
            (PortDirection::Input, SocketType::Image, "Texture"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);

        // Connect noise output -> invert input
        let noise_out = noise_ports[2];
        let invert_in = invert_ports[0];
        let invert_out = invert_ports[1];
        editor.current_graph_mut().connect(noise_out, invert_in).expect("connect");

        let snap = ParamSnapshot {
            floats: HashMap::new(),
            colors: HashMap::new(),
        };
        let result = evaluate(&editor, &snap);

        let noise_tex = &result.textures[&noise_out];
        let invert_tex = &result.textures[&invert_out];

        // Every pixel of inverted should be 255 - noise for RGB channels
        for i in 0..TEX_SIZE * TEX_SIZE {
            assert_eq!(invert_tex.data[i][0], 255 - noise_tex.data[i][0], "R channel mismatch at {i}");
            assert_eq!(invert_tex.data[i][1], 255 - noise_tex.data[i][1], "G channel mismatch at {i}");
            assert_eq!(invert_tex.data[i][2], 255 - noise_tex.data[i][2], "B channel mismatch at {i}");
        }
    }

    #[wasm_bindgen_test]
    fn eval_disconnected_uses_default() {
        let mut editor = GraphEditor::new();
        // Invert with no input should use black (TextureBuffer::new() = all [0,0,0,255])
        let (_invert_id, invert_ports) = make_node(&mut editor, "Invert", "invert", &[
            (PortDirection::Input, SocketType::Image, "Texture"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);
        let invert_out = invert_ports[1];

        let snap = ParamSnapshot {
            floats: HashMap::new(),
            colors: HashMap::new(),
        };
        let result = evaluate(&editor, &snap);

        let tex = &result.textures[&invert_out];
        // invert of black [0,0,0,255] -> [255,255,255,255]
        for px in &tex.data {
            assert_eq!(*px, [255, 255, 255, 255], "invert of black should be white");
        }
    }

    #[wasm_bindgen_test]
    fn eval_topo_order() {
        // A(noise) -> B(invert) -> C(invert): double invert = original
        let mut editor = GraphEditor::new();
        let (_a_id, a_ports) = make_node(&mut editor, "Noise", "noise", &[
            (PortDirection::Input, SocketType::Float, "Scale"),
            (PortDirection::Input, SocketType::Float, "Seed"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);
        let (_b_id, b_ports) = make_node(&mut editor, "Invert1", "invert", &[
            (PortDirection::Input, SocketType::Image, "Texture"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);
        let (_c_id, c_ports) = make_node(&mut editor, "Invert2", "invert", &[
            (PortDirection::Input, SocketType::Image, "Texture"),
            (PortDirection::Output, SocketType::Image, "Texture"),
        ]);

        let a_out = a_ports[2];
        let b_in = b_ports[0];
        let b_out = b_ports[1];
        let c_in = c_ports[0];
        let c_out = c_ports[1];

        editor.current_graph_mut().connect(a_out, b_in).expect("connect A->B");
        editor.current_graph_mut().connect(b_out, c_in).expect("connect B->C");

        let snap = ParamSnapshot {
            floats: HashMap::new(),
            colors: HashMap::new(),
        };
        let result = evaluate(&editor, &snap);

        let a_tex = &result.textures[&a_out];
        let c_tex = &result.textures[&c_out];
        assert_eq!(a_tex.data, c_tex.data, "double invert through chain must equal original");
    }

    #[wasm_bindgen_test]
    fn eval_empty_graph() {
        let editor = GraphEditor::new();
        let snap = ParamSnapshot {
            floats: HashMap::new(),
            colors: HashMap::new(),
        };
        let result = evaluate(&editor, &snap);
        assert!(result.textures.is_empty(), "empty graph must produce empty result");
    }
}
