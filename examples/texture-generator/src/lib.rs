#![cfg_attr(test, allow(dead_code, unused_imports))]

mod eval;
mod nodes;
mod params;
mod preview;
mod reactive_eval;
pub(crate) mod texture;

use std::rc::Rc;

use dominator::html;
use futures_signals::signal::SignalExt;
use futures_signals::signal_vec::SignalVecExt;
use nodegraph_core::graph::node::{CustomBodyHeight, NodeTypeId};
use nodegraph_core::{EntityId, PortDirection, SocketType};
use nodegraph_render::{render_graph_editor, GraphSignals};
use wasm_bindgen::prelude::*;

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

    // Node/connection additions and removals are picked up by the
    // reactive-eval reconciliation watcher subscribing to `node_list` /
    // `connection_list` signals. The only graph mutations that can't be
    // handled purely reactively are group/ungroup: they carry an ephemeral
    // old→new port-ID mapping that the ParamStore needs to migrate its
    // state by. Those remain as callbacks.
    {
        let params2 = params.clone();
        gs.set_on_group(move |_group_id, _sub_id, port_map| {
            params2.migrate_ports(&port_map);
        });
    }
    {
        let params2 = params.clone();
        gs.set_on_ungroup(move |port_map| {
            params2.migrate_ports(&port_map);
        });
    }

    // Watch new nodes for CustomBodyHeight insertion based on their type.
    // This is render-affecting state that must be set before the node renders;
    // using a signal watcher keeps it in lock-step with reactive-eval setup.
    spawn_body_height_watcher(&gs);

    // Reconciliation watcher for undo/redo/delete/group — subscribes to
    // node_list + connection_list MutableVec signals and reconciles on any
    // structural change.
    reval.spawn_reconciliation_watcher();

    dominator::append_dom(
        &dominator::body(),
        html!("div", {
            .style("width", "100%")
            .style("height", "100%")
            .child(render_graph_editor(gs))
        }),
    );
}

struct SceneNodes {
    all_node_ids: Vec<EntityId>,
}

fn build_default_scene(gs: &Rc<GraphSignals>, params: &Rc<ParamStore>) -> SceneNodes {
    use nodegraph_core::graph::node::NodeHeader;

    let mut all_node_ids: Vec<EntityId> = Vec::new();

    // ===== Minecraft-style grass block (ungrouped) =====
    // Graph shape:
    //   GrassNoise → GrassColorize ┬──────────────────► BlockPreview.Top
    //                              └── Blend.A
    //   DirtNoise  → DirtColorize → DirtBC             ── Blend.B
    //   Gradient   → Mix(factor) ← JaggedNoise → Threshold ─ Blend.Mask
    //   Blend                                             ── BlockPreview.Side
    //
    // The vertical gradient provides a clean top/bottom split (white=pick A=grass,
    // black=pick B=dirt). Mixing in a high-frequency jagged noise distorts the
    // boundary, and the threshold hardens it into a crisp mask. The side face
    // then shows grass draping over dirt with an organic silhouette.

    let noise_ports = || {
        vec![
            (PortDirection::Input, SocketType::Float, "Scale".to_string()),
            (PortDirection::Input, SocketType::Float, "Seed".to_string()),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ]
    };
    let colorize_ports = || {
        vec![
            (
                PortDirection::Input,
                SocketType::Image,
                "Texture".to_string(),
            ),
            (PortDirection::Input, SocketType::Color, "Tint".to_string()),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ]
    };

    // --- grass column ---
    let (grass_n_id, grass_n_ports) =
        gs.add_node_typed("Noise", Some("noise"), (60.0, 40.0), noise_ports());
    params.get_float(grass_n_ports[0], 5.5); // Scale (higher = finer grain)
    params.get_float(grass_n_ports[1], 7.0); // Seed

    let (grass_c_id, grass_c_ports) = gs.add_node_typed(
        "Colorize",
        Some("colorize"),
        (280.0, 40.0),
        colorize_ports(),
    );
    params.get_color(grass_c_ports[1], [91, 153, 56, 255]);

    // --- dirt column ---
    // Ordering: Noise → BC → Colorize. The BC step pushes noise values up so
    // the multiply in Colorize doesn't crush the dirt into near-black.
    let (dirt_n_id, dirt_n_ports) =
        gs.add_node_typed("Noise", Some("noise"), (60.0, 280.0), noise_ports());
    params.get_float(dirt_n_ports[0], 4.5); // finer pebbly dirt
    params.get_float(dirt_n_ports[1], 13.0);

    let (dirt_bc_id, dirt_bc_ports) = gs.add_node_typed(
        "Brightness/Contrast",
        Some("brightness_contrast"),
        (280.0, 280.0),
        vec![
            (
                PortDirection::Input,
                SocketType::Image,
                "Texture".to_string(),
            ),
            (
                PortDirection::Input,
                SocketType::Float,
                "Brightness".to_string(),
            ),
            (
                PortDirection::Input,
                SocketType::Float,
                "Contrast".to_string(),
            ),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );
    params.get_float(dirt_bc_ports[1], 0.35); // brighten noise before tint
    params.get_float(dirt_bc_ports[2], 0.4); // punch up contrast

    let (dirt_c_id, dirt_c_ports) = gs.add_node_typed(
        "Colorize",
        Some("colorize"),
        (500.0, 280.0),
        colorize_ports(),
    );
    params.get_color(dirt_c_ports[1], [168, 120, 78, 255]);

    // --- mask column ---
    let (grad_id, grad_ports) = gs.add_node_typed(
        "Gradient",
        Some("gradient"),
        (60.0, 520.0),
        vec![
            (
                PortDirection::Input,
                SocketType::Color,
                "Color A".to_string(),
            ),
            (
                PortDirection::Input,
                SocketType::Color,
                "Color B".to_string(),
            ),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );
    // Blend convention is lerp(A, B, mask_luminance): mask=0 picks A (grass),
    // mask=1 picks B (dirt). So the gradient needs black at the TOP of the
    // image (mask=0 → grass) and white at the BOTTOM (mask=1 → dirt).
    params.get_color(grad_ports[0], [0, 0, 0, 255]); // Color A = top = black
    params.get_color(grad_ports[1], [255, 255, 255, 255]); // Color B = bottom = white

    let (jag_n_id, jag_n_ports) =
        gs.add_node_typed("Noise", Some("noise"), (60.0, 760.0), noise_ports());
    params.get_float(jag_n_ports[0], 7.0); // chunkier grass drape
    params.get_float(jag_n_ports[1], 99.0);

    let (mix_id, mix_ports) = gs.add_node_typed(
        "Mix",
        Some("mix"),
        (320.0, 640.0),
        vec![
            (PortDirection::Input, SocketType::Image, "A".to_string()),
            (PortDirection::Input, SocketType::Image, "B".to_string()),
            (
                PortDirection::Input,
                SocketType::Float,
                "Factor".to_string(),
            ),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );
    params.get_float(mix_ports[2], 0.45); // let jagged noise dominate the gradient

    let (thresh_id, thresh_ports) = gs.add_node_typed(
        "Threshold",
        Some("threshold"),
        (560.0, 640.0),
        vec![
            (
                PortDirection::Input,
                SocketType::Image,
                "Texture".to_string(),
            ),
            (PortDirection::Input, SocketType::Float, "Level".to_string()),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );
    params.get_float(thresh_ports[1], 0.3); // lower threshold → more pixels
                                            // end up above it in luminance → more dirt, less grass (Minecraft-proportion drape)

    // --- blend + preview ---
    let (blend_id, blend_ports) = gs.add_node_typed(
        "Blend",
        Some("blend"),
        (780.0, 400.0),
        vec![
            (PortDirection::Input, SocketType::Image, "A".to_string()),
            (PortDirection::Input, SocketType::Image, "B".to_string()),
            (PortDirection::Input, SocketType::Image, "Mask".to_string()),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );

    let (block_preview_id, block_preview_ports) = gs.add_node_typed(
        "Block Preview",
        Some("block_preview"),
        (1080.0, 40.0),
        vec![
            (PortDirection::Input, SocketType::Image, "Top".to_string()),
            (PortDirection::Input, SocketType::Image, "Side".to_string()),
        ],
    );

    for (nid, tid) in [
        (grass_n_id, "noise"),
        (grass_c_id, "colorize"),
        (dirt_n_id, "noise"),
        (dirt_c_id, "colorize"),
        (dirt_bc_id, "brightness_contrast"),
        (grad_id, "gradient"),
        (jag_n_id, "noise"),
        (mix_id, "mix"),
        (thresh_id, "threshold"),
        (blend_id, "blend"),
        (block_preview_id, "block_preview"),
    ] {
        set_body_height_for_node(gs, nid, tid);
    }

    // Wiring
    let _ = gs.connect_ports(grass_n_ports[2], grass_c_ports[0]); // GrassNoise → GrassColorize.Texture
    let _ = gs.connect_ports(dirt_n_ports[2], dirt_bc_ports[0]); // DirtNoise → DirtBC.Texture
    let _ = gs.connect_ports(dirt_bc_ports[3], dirt_c_ports[0]); // DirtBC → DirtColorize.Texture
    let _ = gs.connect_ports(grad_ports[2], mix_ports[0]); // Gradient → Mix.A
    let _ = gs.connect_ports(jag_n_ports[2], mix_ports[1]); // JaggedNoise → Mix.B
    let _ = gs.connect_ports(mix_ports[3], thresh_ports[0]); // Mix → Threshold.Texture
    let _ = gs.connect_ports(grass_c_ports[2], blend_ports[0]); // GrassColorize → Blend.A
    let _ = gs.connect_ports(dirt_c_ports[2], blend_ports[1]); // DirtColorize → Blend.B
    let _ = gs.connect_ports(thresh_ports[2], blend_ports[2]); // Threshold → Blend.Mask
    let _ = gs.connect_ports(grass_c_ports[2], block_preview_ports[0]); // Grass → BlockPreview.Top
    let _ = gs.connect_ports(blend_ports[3], block_preview_ports[1]); // Blend → BlockPreview.Side

    gs.full_sync_pub();

    for nid in [
        grass_n_id,
        grass_c_id,
        dirt_n_id,
        dirt_c_id,
        dirt_bc_id,
        grad_id,
        jag_n_id,
        mix_id,
        thresh_id,
        blend_id,
        block_preview_id,
    ] {
        all_node_ids.push(nid);
    }

    // ===== Grass group =====
    let (grass_noise_id, grass_noise_ports) = gs.add_node_typed(
        "Noise",
        Some("noise"),
        (100.0, 400.0),
        vec![
            (PortDirection::Input, SocketType::Float, "Scale".to_string()),
            (PortDirection::Input, SocketType::Float, "Seed".to_string()),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );

    let (grass_colorize_id, grass_colorize_ports) = gs.add_node_typed(
        "Colorize",
        Some("colorize"),
        (300.0, 400.0),
        vec![
            (
                PortDirection::Input,
                SocketType::Image,
                "Texture".to_string(),
            ),
            (PortDirection::Input, SocketType::Color, "Tint".to_string()),
            (
                PortDirection::Output,
                SocketType::Image,
                "Texture".to_string(),
            ),
        ],
    );

    let (grass_color_id, grass_color_ports) = gs.add_node_typed(
        "Solid Color",
        Some("solid_color"),
        (60.0, 1040.0),
        vec![(
            PortDirection::Output,
            SocketType::Color,
            "Color".to_string(),
        )],
    );
    params.get_color(grass_color_ports[0], [74, 160, 46, 255]);

    let (grass_tiled_id, grass_tiled_ports) = gs.add_node_typed(
        "Tiled Preview",
        Some("tiled_preview"),
        (380.0, 1040.0),
        vec![(
            PortDirection::Input,
            SocketType::Image,
            "Texture".to_string(),
        )],
    );

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

        let grass_header = NodeHeader {
            title: "Grass".to_string(),
            color: [50, 120, 40],
            collapsed: false,
        };
        gs.with_graph_mut(|g| {
            if let Some(h) = g.world.get_mut::<NodeHeader>(gid) {
                *h = grass_header.clone();
            }
            if let Some(p) = g
                .world
                .get_mut::<nodegraph_core::graph::node::NodePosition>(gid)
            {
                p.x = 200.0;
                p.y = 1040.0;
            }
        });
        if let Some(s) = gs.get_node_header_signal(gid) {
            s.set(grass_header);
        }
        if let Some(s) = gs.get_node_position_signal(gid) {
            s.set((200.0, 1040.0));
        }
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
    use nodegraph_core::graph::port::{PortDirection, PortLabel};

    for (nid, header) in graph.world.query::<NodeHeader>() {
        if header.title != node_title {
            continue;
        }
        for &pid in graph.node_ports(nid) {
            if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) {
                continue;
            }
            let label = graph
                .world
                .get::<PortLabel>(pid)
                .map(|l| l.0.as_str())
                .unwrap_or("");
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

/// Subscribes to the node list. Whenever it emits a diff, visits every node
/// with a `NodeTypeId` (i.e. spawned from the registry) and inserts the
/// matching `CustomBodyHeight` component. Idempotent: re-inserting the same
/// component is cheap and reaches a fixed point.
fn spawn_body_height_watcher(gs: &Rc<GraphSignals>) {
    use nodegraph_core::graph::node::NodeHeader;
    let gs2 = gs.clone();
    wasm_bindgen_futures::spawn_local(async move {
        gs2.node_list
            .signal_vec_cloned()
            .to_signal_cloned()
            .for_each(move |_| {
                let ids: Vec<(EntityId, String)> = gs2.with_graph(|g| {
                    g.world
                        .query::<NodeHeader>()
                        .filter_map(|(id, _)| {
                            g.world.get::<NodeTypeId>(id).map(|t| (id, t.0.clone()))
                        })
                        .collect()
                });
                for (id, tid) in ids {
                    set_body_height_for_node(&gs2, id, &tid);
                }
                async {}
            })
            .await;
    });
}

fn body_height_for_type(type_id: &str) -> f64 {
    match type_id {
        "preview" | "tiled_preview" | "iso_preview" | "block_preview" => OUTPUT_BODY_H,
        "checker"
        | "noise"
        | "gradient"
        | "brick"
        | "mix"
        | "blend"
        | "brightness_contrast"
        | "threshold"
        | "invert"
        | "colorize" => PREVIEW_BODY_H,
        _ => 0.0,
    }
}

fn set_body_heights(gs: &Rc<GraphSignals>, scene: &SceneNodes) {
    gs.with_graph_mut(|graph| {
        for &nid in &scene.all_node_ids {
            let type_id = graph
                .world
                .get::<nodegraph_core::graph::node::NodeTypeId>(nid)
                .map(|t| t.0.clone())
                .unwrap_or_default();
            let h = body_height_for_type(&type_id);
            if h > 0.0 {
                graph.world.insert(nid, CustomBodyHeight(h));
            }
        }
    });
}
