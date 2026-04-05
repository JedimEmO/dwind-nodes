use std::collections::HashMap;
use std::rc::Rc;

use nodegraph_core::EntityId;
use nodegraph_core::graph::node::NodeTypeId;
use nodegraph_core::graph::port::{PortDirection, PortLabel, PortOwner};
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::graph::group::SubgraphRoot;
use nodegraph_core::graph::{GroupIOKind, NodeGraph};
use nodegraph_render::GraphSignals;

use crate::params::ParamStore;
use crate::texture::{TextureBuffer, TEX_SIZE};

pub struct EvalResult {
    pub textures: HashMap<EntityId, Rc<TextureBuffer>>,
}

/// Evaluate the entire graph (including subgraphs) and return textures keyed by node EntityId.
pub fn evaluate(gs: &Rc<GraphSignals>, params: &Rc<ParamStore>) -> EvalResult {
    let mut textures: HashMap<EntityId, Rc<TextureBuffer>> = HashMap::new();
    let mut colors: HashMap<EntityId, [u8; 4]> = HashMap::new();

    let editor = gs.editor.borrow();
    let root_id = editor.root_graph_id();
    let graph = editor.graph(root_id).expect("root graph");

    eval_graph(graph, &editor, params, &mut textures, &mut colors);

    EvalResult { textures }
}

/// Evaluate a single graph level, recursing into groups.
fn eval_graph(
    graph: &NodeGraph,
    editor: &nodegraph_core::graph::GraphEditor,
    params: &Rc<ParamStore>,
    textures: &mut HashMap<EntityId, Rc<TextureBuffer>>,
    colors: &mut HashMap<EntityId, [u8; 4]>,
) {
    let eval_order = graph.eval_order();

    for node_id in eval_order {
        // Check if this is a group node
        if let Some(sub_root) = graph.world.get::<SubgraphRoot>(node_id) {
            eval_group_node(node_id, sub_root.0, graph, editor, params, textures, colors);
            continue;
        }

        let type_id = graph.world.get::<NodeTypeId>(node_id)
            .map(|t| t.0.clone())
            .unwrap_or_default();

        // Skip Group IO nodes — they're handled by eval_group_node
        if graph.world.get::<GroupIOKind>(node_id).is_some() {
            continue;
        }

        eval_node(node_id, &type_id, graph, params, textures, colors);
    }
}

/// Evaluate a group node by running its subgraph.
fn eval_group_node(
    group_node_id: EntityId,
    subgraph_id: EntityId,
    parent_graph: &NodeGraph,
    editor: &nodegraph_core::graph::GraphEditor,
    params: &Rc<ParamStore>,
    textures: &mut HashMap<EntityId, Rc<TextureBuffer>>,
    colors: &mut HashMap<EntityId, [u8; 4]>,
) {
    let subgraph = match editor.graph(subgraph_id) {
        Some(g) => g,
        None => return,
    };

    // Map group node input values → subgraph Group Input IO node outputs.
    // For each input port on the group node, find the connected upstream value,
    // then find the corresponding IO port in the subgraph and inject the value.
    for &gport in parent_graph.node_ports(group_node_id) {
        let dir = parent_graph.world.get::<PortDirection>(gport).copied().unwrap_or(PortDirection::Output);
        if dir != PortDirection::Input { continue; }

        // Find what's connected to this group input port
        let upstream_color = find_upstream(gport, parent_graph, colors);
        let upstream_tex = find_upstream(gport, parent_graph, textures);

        // Find corresponding IO port in subgraph via io_port_mapping
        // The mapping is (subgraph_id, io_port) → group_port
        // We need the reverse: group_port → io_port
        for (&(sid, io_port), &mapped_gport) in &editor.io_port_mapping {
            if sid != subgraph_id || mapped_gport != gport { continue; }

            // io_port is an output port on a Group Input IO node in the subgraph.
            // Find the IO node that owns it.
            if let Some(io_owner) = subgraph.world.get::<PortOwner>(io_port).map(|o| o.0) {
                if let Some(c) = upstream_color {
                    colors.insert(io_owner, c);
                }
                if let Some(ref t) = upstream_tex {
                    textures.insert(io_owner, t.clone());
                }
            }
        }
    }

    // Evaluate the subgraph
    let sub_order = subgraph.eval_order();
    for sub_node_id in sub_order {
        if subgraph.world.get::<GroupIOKind>(sub_node_id).is_some() {
            // Group Input IO nodes: their values were injected above
            // Group Output IO nodes: we read from their connected input below
            continue;
        }
        if let Some(sub_root) = subgraph.world.get::<SubgraphRoot>(sub_node_id) {
            eval_group_node(sub_node_id, sub_root.0, subgraph, editor, params, textures, colors);
            continue;
        }
        let type_id = subgraph.world.get::<NodeTypeId>(sub_node_id)
            .map(|t| t.0.clone())
            .unwrap_or_default();
        eval_node(sub_node_id, &type_id, subgraph, params, textures, colors);
    }

    // Map subgraph Group Output IO node inputs → group node output values.
    for &gport in parent_graph.node_ports(group_node_id) {
        let dir = parent_graph.world.get::<PortDirection>(gport).copied().unwrap_or(PortDirection::Input);
        if dir != PortDirection::Output { continue; }

        for (&(sid, io_port), &mapped_gport) in &editor.io_port_mapping {
            if sid != subgraph_id || mapped_gport != gport { continue; }

            // io_port is an input port on a Group Output IO node.
            // Find what's connected to it in the subgraph.
            if let Some(t) = find_upstream(io_port, subgraph, textures) {
                textures.insert(group_node_id, t);
            }
            if let Some(c) = find_upstream(io_port, subgraph, colors) {
                colors.insert(group_node_id, c);
            }
        }
    }
}

/// Find the upstream source node for an input port and look up its value in the given map.
fn find_upstream<T: Clone>(
    port_id: EntityId, graph: &NodeGraph, values: &HashMap<EntityId, T>,
) -> Option<T> {
    for &conn_id in graph.port_connections(port_id) {
        let ep = graph.world.get::<ConnectionEndpoints>(conn_id)?;
        if ep.target_port != port_id { continue; }
        let src_node = graph.world.get::<PortOwner>(ep.source_port)?.0;
        if let Some(v) = values.get(&src_node) {
            return Some(v.clone());
        }
    }
    None
}

/// Evaluate a single non-group node.
fn eval_node(
    node_id: EntityId,
    type_id: &str,
    graph: &NodeGraph,
    params: &Rc<ParamStore>,
    textures: &mut HashMap<EntityId, Rc<TextureBuffer>>,
    colors: &mut HashMap<EntityId, [u8; 4]>,
) {
    let get_input_texture = |label: &str| -> Option<Rc<TextureBuffer>> {
        for &pid in graph.node_ports(node_id) {
            let dir = graph.world.get::<PortDirection>(pid).copied()?;
            if dir != PortDirection::Input { continue; }
            let pl = graph.world.get::<PortLabel>(pid).map(|l| l.0.as_str())?;
            if pl != label { continue; }
            for &conn_id in graph.port_connections(pid) {
                let ep = graph.world.get::<ConnectionEndpoints>(conn_id)?;
                if ep.target_port != pid { continue; }
                let src_node = graph.world.get::<PortOwner>(ep.source_port)?.0;
                return textures.get(&src_node).cloned();
            }
        }
        None
    };

    let get_float = |label: &str| -> f64 {
        for &pid in graph.node_ports(node_id) {
            let dir = graph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Output);
            if dir != PortDirection::Input { continue; }
            let pl = graph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
            if pl != label { continue; }
            return params.get_float(pid, crate::params::default_float(type_id, label)).get();
        }
        0.0
    };

    let get_color = |label: &str| -> [u8; 4] {
        for &pid in graph.node_ports(node_id) {
            let dir = graph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Output);
            if dir != PortDirection::Input { continue; }
            let pl = graph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
            if pl != label { continue; }
            // Check connected upstream color
            for &conn_id in graph.port_connections(pid) {
                if let Some(ep) = graph.world.get::<ConnectionEndpoints>(conn_id) {
                    if ep.target_port != pid { continue; }
                    if let Some(src_node) = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0) {
                        if let Some(&c) = colors.get(&src_node) {
                            return c;
                        }
                    }
                }
            }
            return params.get_color(pid, crate::params::default_color(type_id, label)).get();
        }
        [200, 200, 200, 255]
    };

    match type_id {
        "solid_color" => {
            let color = graph.node_ports(node_id).iter()
                .find(|&&pid| graph.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Output))
                .map(|&pid| params.get_color(pid, crate::params::default_color("solid_color", "Color")).get())
                .unwrap_or([200, 200, 200, 255]);
            colors.insert(node_id, color);
        }
        "checker" => { textures.insert(node_id, Rc::new(eval_checker(get_color("Color A"), get_color("Color B"), get_float("Size")))); }
        "noise" => { textures.insert(node_id, Rc::new(eval_noise(get_float("Scale"), get_float("Seed")))); }
        "gradient" => { textures.insert(node_id, Rc::new(eval_gradient(get_color("Color A"), get_color("Color B")))); }
        "brick" => { textures.insert(node_id, Rc::new(eval_brick(get_color("Mortar"), get_color("Brick"), get_float("Rows")))); }
        "mix" => { textures.insert(node_id, Rc::new(eval_mix(get_input_texture("A"), get_input_texture("B"), get_float("Factor")))); }
        "brightness_contrast" => { textures.insert(node_id, Rc::new(eval_brightness_contrast(get_input_texture("Texture"), get_float("Brightness"), get_float("Contrast")))); }
        "threshold" => { textures.insert(node_id, Rc::new(eval_threshold(get_input_texture("Texture"), get_float("Level")))); }
        "invert" => { textures.insert(node_id, Rc::new(eval_invert(get_input_texture("Texture")))); }
        "colorize" => { textures.insert(node_id, Rc::new(eval_colorize(get_input_texture("Texture"), get_color("Tint")))); }
        "preview" | "tiled_preview" | "iso_preview" => {
            if let Some(t) = get_input_texture("Texture") {
                textures.insert(node_id, t);
            } else {
                textures.insert(node_id, Rc::new(TextureBuffer::new()));
            }
        }
        _ => {}
    }
}

// ============================================================
// Texture generation functions
// ============================================================

fn eval_checker(color_a: [u8; 4], color_b: [u8; 4], size: f64) -> TextureBuffer {
    let mut tex = TextureBuffer::new();
    let cell = (size as usize).max(1);
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let checker = ((x / cell) + (y / cell)) % 2 == 0;
            tex.set(x, y, if checker { color_a } else { color_b });
        }
    }
    tex
}

fn eval_noise(scale: f64, seed: f64) -> TextureBuffer {
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

fn value_noise(x: f64, y: f64, seed: u32) -> f64 {
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

fn hash_f(x: i32, y: i32, seed: u32) -> f64 {
    let mut h = seed;
    h ^= x as u32;
    h = h.wrapping_mul(0x45d9f3b);
    h ^= y as u32;
    h = h.wrapping_mul(0x45d9f3b);
    h ^= h >> 16;
    (h & 0xFFFF) as f64 / 65535.0
}

fn eval_gradient(color_a: [u8; 4], color_b: [u8; 4]) -> TextureBuffer {
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

fn eval_brick(mortar: [u8; 4], brick: [u8; 4], rows: f64) -> TextureBuffer {
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

fn eval_mix(a: Option<Rc<TextureBuffer>>, b: Option<Rc<TextureBuffer>>, factor: f64) -> TextureBuffer {
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

fn eval_brightness_contrast(input: Option<Rc<TextureBuffer>>, brightness: f64, contrast: f64) -> TextureBuffer {
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

fn eval_threshold(input: Option<Rc<TextureBuffer>>, level: f64) -> TextureBuffer {
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

fn eval_invert(input: Option<Rc<TextureBuffer>>) -> TextureBuffer {
    let input = input.unwrap_or_else(|| Rc::new(TextureBuffer::new()));
    let mut tex = TextureBuffer::new();
    for i in 0..TEX_SIZE * TEX_SIZE {
        let [r, g, b, a] = input.data[i];
        tex.data[i] = [255 - r, 255 - g, 255 - b, a];
    }
    tex
}

fn eval_colorize(input: Option<Rc<TextureBuffer>>, tint: [u8; 4]) -> TextureBuffer {
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

fn lerp_color(a: [u8; 4], b: [u8; 4], t: f64) -> [u8; 4] {
    [
        (a[0] as f64 + (b[0] as f64 - a[0] as f64) * t) as u8,
        (a[1] as f64 + (b[1] as f64 - a[1] as f64) * t) as u8,
        (a[2] as f64 + (b[2] as f64 - a[2] as f64) * t) as u8,
        (a[3] as f64 + (b[3] as f64 - a[3] as f64) * t) as u8,
    ]
}
