mod eval;
mod nodes;
mod params;
mod preview;
mod texture;

use std::rc::Rc;

use wasm_bindgen::prelude::*;
use dominator::html;
use nodegraph_core::{EntityId, PortDirection, SocketType};
use nodegraph_core::graph::node::CustomBodyHeight;
use nodegraph_render::{GraphSignals, render_graph_editor};

use crate::params::ParamStore;
use crate::preview::{CanvasRegistry, new_canvas_registry};

const PREVIEW_BODY_H: f64 = 90.0;   // nodes with small texture preview
const OUTPUT_BODY_H: f64 = 150.0;    // the Preview output node

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    dwind::stylesheet();

    let gs = GraphSignals::new();
    let params = ParamStore::new();
    let canvases: CanvasRegistry = new_canvas_registry();

    // Set up param change watcher FIRST — before anything creates params,
    // so every Mutable created via get_float/get_color gets a signal watcher.
    {
        let gs2 = gs.clone();
        let p = params.clone();
        let c = canvases.clone();
        params.set_on_change(Rc::new(move || {
            run_evaluation(&gs2, &p, &c);
        }));
    }

    // Register all node types
    {
        let mut reg = gs.registry.borrow_mut();
        nodes::register_all(&mut reg);
    }

    // Port widget callback (float inputs + color pickers on disconnected ports)
    {
        let pw = params::make_port_widget(&params);
        gs.port_widget.borrow_mut().replace(pw);
    }

    // Custom node body callback (canvas previews)
    {
        let cb = preview::make_custom_body(&canvases, &gs, &params);
        gs.custom_node_body.borrow_mut().replace(cb);
    }

    // === Default scene ===
    let scene = build_default_scene(&gs, &params);

    // Set CustomBodyHeight on all texture-producing nodes
    set_body_heights(&gs, &scene);

    // Initial evaluation + preview update
    run_evaluation(&gs, &params, &canvases);

    // Re-evaluate on connection changes
    {
        let gs2 = gs.clone();
        let p = params.clone();
        let c = canvases.clone();
        *gs.on_connect.borrow_mut() = Some(Box::new(move |_, _, _| {
            run_evaluation(&gs2, &p, &c);
        }));
    }
    {
        let gs2 = gs.clone();
        let p = params.clone();
        let c = canvases.clone();
        *gs.on_disconnect.borrow_mut() = Some(Box::new(move |_| {
            run_evaluation(&gs2, &p, &c);
        }));
    }

    // Set CustomBodyHeight on newly spawned nodes (from search menu)
    {
        let gs2 = gs.clone();
        *gs.on_node_spawned.borrow_mut() = Some(Box::new(move |node_id, type_id, _ports| {
            let h = body_height_for_type(type_id);
            if h > 0.0 {
                gs2.with_graph_mut(|g| {
                    g.world.insert(node_id, CustomBodyHeight(h));
                });
            }
        }));
    }

    dominator::append_dom(&dominator::body(), html!("div", {
        .style("width", "100%")
        .style("height", "100%")
        .child(render_graph_editor(gs))
    }));
}

fn run_evaluation(gs: &Rc<GraphSignals>, params: &Rc<ParamStore>, canvases: &CanvasRegistry) {
    let result = eval::evaluate(gs, params);
    preview::update_previews(canvases, &result.textures);
}

struct SceneNodes {
    all_node_ids: Vec<EntityId>,
}

fn build_default_scene(gs: &Rc<GraphSignals>, params: &Rc<ParamStore>) -> SceneNodes {
    use nodegraph_core::graph::node::NodeHeader;

    let mut all_node_ids: Vec<EntityId> = Vec::new();

    // ===== Cobblestone group =====
    // Inner: Noise(scale=3) → Brightness/Contrast(+0.1 brightness, +0.5 contrast) → Colorize
    // Gives chunky stone-like blocks with high contrast

    let (cobble_noise_id, cobble_noise_ports) = gs.add_node_typed("Noise", Some("noise"), (100.0, 80.0), vec![
        (PortDirection::Input, SocketType::Float, "Scale".to_string()),
        (PortDirection::Input, SocketType::Float, "Seed".to_string()),
        (PortDirection::Output, SocketType::Image, "Texture".to_string()),
    ]);

    let (cobble_bc_id, cobble_bc_ports) = gs.add_node_typed("Brightness/Contrast", Some("brightness_contrast"), (300.0, 80.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
        (PortDirection::Input, SocketType::Float, "Brightness".to_string()),
        (PortDirection::Input, SocketType::Float, "Contrast".to_string()),
        (PortDirection::Output, SocketType::Image, "Texture".to_string()),
    ]);

    let (cobble_colorize_id, cobble_colorize_ports) = gs.add_node_typed("Colorize", Some("colorize"), (500.0, 80.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
        (PortDirection::Input, SocketType::Color, "Tint".to_string()),
        (PortDirection::Output, SocketType::Image, "Texture".to_string()),
    ]);

    // Solid Color for stone tint (outside group)
    let (cobble_color_id, cobble_color_ports) = gs.add_node_typed("Solid Color", Some("solid_color"), (50.0, 50.0), vec![
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
    ]);
    params.get_color(cobble_color_ports[0], [138, 138, 122, 255]); // stone gray

    // Iso Preview (outside group)
    let (cobble_iso_id, cobble_iso_ports) = gs.add_node_typed("Iso Preview", Some("iso_preview"), (900.0, 30.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
    ]);

    // Set body heights on inner nodes BEFORE grouping (so they're preserved in the subgraph)
    set_body_height_for_node(gs, cobble_noise_id, "noise");
    set_body_height_for_node(gs, cobble_bc_id, "brightness_contrast");
    set_body_height_for_node(gs, cobble_colorize_id, "colorize");

    // Wire inner chain + external connections
    let _ = gs.connect_ports(cobble_noise_ports[2], cobble_bc_ports[0]);      // Noise → B/C
    let _ = gs.connect_ports(cobble_bc_ports[3], cobble_colorize_ports[0]);   // B/C → Colorize
    let _ = gs.connect_ports(cobble_color_ports[0], cobble_colorize_ports[1]); // SolidColor → Colorize.Tint
    let _ = gs.connect_ports(cobble_colorize_ports[2], cobble_iso_ports[0]);  // Colorize → IsoPreview

    // Group the inner nodes
    let cobble_group = {
        let mut editor = gs.editor.borrow_mut();
        editor.group_nodes(&[cobble_noise_id, cobble_bc_id, cobble_colorize_id])
    };
    gs.full_sync_pub();

    if let Some((gid, sub_id)) = cobble_group {
        let cobble_header = NodeHeader { title: "Cobblestone".to_string(), color: [120, 90, 60], collapsed: false };
        gs.with_graph_mut(|g| {
            if let Some(h) = g.world.get_mut::<NodeHeader>(gid) {
                *h = cobble_header.clone();
            }
            if let Some(p) = g.world.get_mut::<nodegraph_core::graph::node::NodePosition>(gid) {
                p.x = 400.0;
                p.y = 50.0;
            }
        });
        if let Some(s) = gs.get_node_header_signal(gid) { s.set(cobble_header); }
        if let Some(s) = gs.get_node_position_signal(gid) { s.set((400.0, 50.0)); }
        // Set params on the NEW ports inside the subgraph
        let editor = gs.editor.borrow();
        if let Some(sub) = editor.graph(sub_id) {
            set_subgraph_float(sub, params, "Noise", "Scale", 3.0);
            set_subgraph_float(sub, params, "Noise", "Seed", 1.0);
            set_subgraph_float(sub, params, "Brightness/Contrast", "Brightness", 0.0);
            set_subgraph_float(sub, params, "Brightness/Contrast", "Contrast", 0.64);
        }
        drop(editor);
        all_node_ids.push(gid);
    }
    all_node_ids.push(cobble_color_id);
    all_node_ids.push(cobble_iso_id);

    // ===== Grass group =====
    // Inner: Noise(scale=8, seed=42) → Colorize
    // Fine organic noise tinted green

    let (grass_noise_id, grass_noise_ports) = gs.add_node_typed("Noise", Some("noise"), (100.0, 400.0), vec![
        (PortDirection::Input, SocketType::Float, "Scale".to_string()),
        (PortDirection::Input, SocketType::Float, "Seed".to_string()),
        (PortDirection::Output, SocketType::Image, "Texture".to_string()),
    ]);

    let (grass_colorize_id, grass_colorize_ports) = gs.add_node_typed("Colorize", Some("colorize"), (300.0, 400.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
        (PortDirection::Input, SocketType::Color, "Tint".to_string()),
        (PortDirection::Output, SocketType::Image, "Texture".to_string()),
    ]);

    // Solid Color for grass tint (outside group)
    let (grass_color_id, grass_color_ports) = gs.add_node_typed("Solid Color", Some("solid_color"), (50.0, 350.0), vec![
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
    ]);
    params.get_color(grass_color_ports[0], [74, 160, 46, 255]); // grass green

    // Tiled Preview (outside group)
    let (grass_tiled_id, grass_tiled_ports) = gs.add_node_typed("Tiled Preview", Some("tiled_preview"), (900.0, 300.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
    ]);

    // Set body heights before grouping
    set_body_height_for_node(gs, grass_noise_id, "noise");
    set_body_height_for_node(gs, grass_colorize_id, "colorize");

    // Wire
    let _ = gs.connect_ports(grass_noise_ports[2], grass_colorize_ports[0]);   // Noise → Colorize
    let _ = gs.connect_ports(grass_color_ports[0], grass_colorize_ports[1]);   // SolidColor → Colorize.Tint
    let _ = gs.connect_ports(grass_colorize_ports[2], grass_tiled_ports[0]);   // Colorize → TiledPreview

    // Group inner nodes
    let grass_group = {
        let mut editor = gs.editor.borrow_mut();
        editor.group_nodes(&[grass_noise_id, grass_colorize_id])
    };
    gs.full_sync_pub();

    if let Some((gid, sub_id)) = grass_group {
        let grass_header = NodeHeader { title: "Grass".to_string(), color: [50, 120, 40], collapsed: false };
        gs.with_graph_mut(|g| {
            if let Some(h) = g.world.get_mut::<NodeHeader>(gid) {
                *h = grass_header.clone();
            }
            if let Some(p) = g.world.get_mut::<nodegraph_core::graph::node::NodePosition>(gid) {
                p.x = 400.0;
                p.y = 320.0;
            }
        });
        if let Some(s) = gs.get_node_header_signal(gid) { s.set(grass_header); }
        if let Some(s) = gs.get_node_position_signal(gid) { s.set((400.0, 320.0)); }
        let editor = gs.editor.borrow();
        if let Some(sub) = editor.graph(sub_id) {
            set_subgraph_float(sub, params, "Noise", "Scale", 17.5);
            set_subgraph_float(sub, params, "Noise", "Seed", 42.0);
        }
        drop(editor);
        all_node_ids.push(gid);
    }
    all_node_ids.push(grass_color_id);
    all_node_ids.push(grass_tiled_id);

    SceneNodes { all_node_ids }
}

/// Find a port inside a subgraph by node title + port label, and set its float param.
fn set_subgraph_float(
    graph: &nodegraph_core::graph::NodeGraph,
    params: &Rc<ParamStore>,
    node_title: &str,
    port_label: &str,
    value: f64,
) {
    use nodegraph_core::graph::node::NodeHeader;
    use nodegraph_core::graph::port::{PortLabel, PortDirection};

    for (nid, header) in graph.world.query::<NodeHeader>() {
        if header.title != node_title { continue; }
        for &pid in graph.node_ports(nid) {
            if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }
            let label = graph.world.get::<PortLabel>(pid).map(|l| l.0.as_str()).unwrap_or("");
            if label == port_label {
                params.get_float(pid, value).set(value);
            }
        }
    }
}

fn set_body_height_for_node(gs: &Rc<GraphSignals>, node_id: EntityId, type_id: &str) {
    let h = body_height_for_type(type_id);
    if h > 0.0 {
        gs.with_graph_mut(|g| {
            g.world.insert(node_id, CustomBodyHeight(h));
        });
    }
}

fn body_height_for_type(type_id: &str) -> f64 {
    match type_id {
        "preview" | "tiled_preview" | "iso_preview" => OUTPUT_BODY_H,
        "checker" | "noise" | "gradient" | "brick" |
        "mix" | "brightness_contrast" | "threshold" | "invert" | "colorize" => PREVIEW_BODY_H,
        _ => 0.0,
    }
}

fn set_body_heights(gs: &Rc<GraphSignals>, scene: &SceneNodes) {
    gs.with_graph_mut(|graph| {
        for &nid in &scene.all_node_ids {
            let type_id = graph.world.get::<nodegraph_core::graph::node::NodeTypeId>(nid)
                .map(|t| t.0.clone())
                .unwrap_or_default();
            let h = body_height_for_type(&type_id);
            if h > 0.0 {
                graph.world.insert(nid, CustomBodyHeight(h));
            }
        }
    });
}

