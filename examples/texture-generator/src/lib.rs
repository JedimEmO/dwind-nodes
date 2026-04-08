mod eval;
mod nodes;
mod params;
mod preview;
mod reactive_eval;
pub(crate) mod texture;

use std::rc::Rc;

use wasm_bindgen::prelude::*;
use dominator::html;
use nodegraph_core::{EntityId, PortDirection, SocketType};
use nodegraph_core::graph::node::CustomBodyHeight;
use nodegraph_render::{GraphSignals, render_graph_editor};

use crate::params::ParamStore;
use crate::reactive_eval::ReactiveEval;

const PREVIEW_BODY_H: f64 = 90.0;
const OUTPUT_BODY_H: f64 = 150.0;

#[cfg(not(test))]
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    dwind::stylesheet();

    let gs = GraphSignals::new();
    let params = ParamStore::new();

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

    // Create reactive evaluation layer
    let reval = ReactiveEval::new(gs.clone(), params.clone());

    // Custom node body callback (canvas previews — each watches its own signal)
    {
        let cb = preview::make_custom_body(&reval);
        gs.custom_node_body.borrow_mut().replace(cb);
    }

    // === Default scene ===
    let scene = build_default_scene(&gs, &params);
    set_body_heights(&gs, &scene);

    // Wire reactive eval to current graph state
    reval.initial_setup();

    // on_connect / on_disconnect delegate to ReactiveEval
    {
        let reval2 = reval.clone();
        *gs.on_connect.borrow_mut() = Some(Box::new(move |src, tgt, conn| {
            reval2.handle_connect(src, tgt, conn);
        }));
    }
    {
        let reval2 = reval.clone();
        *gs.on_disconnect.borrow_mut() = Some(Box::new(move |conn| {
            reval2.handle_disconnect(conn);
        }));
    }

    // Set CustomBodyHeight on newly spawned nodes + register in reactive eval
    {
        let gs2 = gs.clone();
        let reval2 = reval.clone();
        *gs.on_node_spawned.borrow_mut() = Some(Box::new(move |node_id, type_id, _ports| {
            let h = body_height_for_type(type_id);
            if h > 0.0 {
                gs2.with_graph_mut(|g| {
                    g.world.insert(node_id, CustomBodyHeight(h));
                });
            }
            reval2.setup_node(node_id);
        }));
    }

    // Reconciliation watcher for undo/redo/delete/group
    reval.spawn_reconciliation_watcher();

    dominator::append_dom(&dominator::body(), html!("div", {
        .style("width", "100%")
        .style("height", "100%")
        .child(render_graph_editor(gs))
    }));
}

struct SceneNodes {
    all_node_ids: Vec<EntityId>,
}

fn build_default_scene(gs: &Rc<GraphSignals>, params: &Rc<ParamStore>) -> SceneNodes {
    use nodegraph_core::graph::node::NodeHeader;

    let mut all_node_ids: Vec<EntityId> = Vec::new();

    // ===== Cobblestone group =====
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

    let (cobble_color_id, cobble_color_ports) = gs.add_node_typed("Solid Color", Some("solid_color"), (50.0, 50.0), vec![
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
    ]);
    params.get_color(cobble_color_ports[0], [138, 138, 122, 255]);

    let (cobble_iso_id, cobble_iso_ports) = gs.add_node_typed("Iso Preview", Some("iso_preview"), (900.0, 30.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
    ]);

    set_body_height_for_node(gs, cobble_noise_id, "noise");
    set_body_height_for_node(gs, cobble_bc_id, "brightness_contrast");
    set_body_height_for_node(gs, cobble_colorize_id, "colorize");

    let _ = gs.connect_ports(cobble_noise_ports[2], cobble_bc_ports[0]);
    let _ = gs.connect_ports(cobble_bc_ports[3], cobble_colorize_ports[0]);
    let _ = gs.connect_ports(cobble_color_ports[0], cobble_colorize_ports[1]);
    let _ = gs.connect_ports(cobble_colorize_ports[2], cobble_iso_ports[0]);

    let cobble_group = {
        let mut editor = gs.editor.borrow_mut();
        editor.group_nodes(&[cobble_noise_id, cobble_bc_id, cobble_colorize_id])
    };
    gs.full_sync_pub();

    if let Some((gid, sub_id, port_map)) = cobble_group {
        // Migrate param values from old port IDs to new subgraph port IDs
        params.migrate_ports(&port_map);

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

    let (grass_color_id, grass_color_ports) = gs.add_node_typed("Solid Color", Some("solid_color"), (50.0, 350.0), vec![
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
    ]);
    params.get_color(grass_color_ports[0], [74, 160, 46, 255]);

    let (grass_tiled_id, grass_tiled_ports) = gs.add_node_typed("Tiled Preview", Some("tiled_preview"), (900.0, 300.0), vec![
        (PortDirection::Input, SocketType::Image, "Texture".to_string()),
    ]);

    set_body_height_for_node(gs, grass_noise_id, "noise");
    set_body_height_for_node(gs, grass_colorize_id, "colorize");

    let _ = gs.connect_ports(grass_noise_ports[2], grass_colorize_ports[0]);
    let _ = gs.connect_ports(grass_color_ports[0], grass_colorize_ports[1]);
    let _ = gs.connect_ports(grass_colorize_ports[2], grass_tiled_ports[0]);

    let grass_group = {
        let mut editor = gs.editor.borrow_mut();
        editor.group_nodes(&[grass_noise_id, grass_colorize_id])
    };
    gs.full_sync_pub();

    if let Some((gid, sub_id, port_map)) = grass_group {
        params.migrate_ports(&port_map);

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
