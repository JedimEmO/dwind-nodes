#![cfg_attr(test, allow(dead_code, unused_imports))]

#[macro_use]
extern crate dwind_macros;

mod computations;
mod eval;
mod nodes;
mod params;
mod preview;
pub(crate) mod texture;

use std::rc::Rc;

use dominator::html;
use dwind::prelude::*;
use nodegraph_core::{PortDirection, SocketType};
use nodegraph_render::{render_graph_editor, GraphSignals};
use nodegraph_runtime::prelude::{ConstNode, NodeComputation, ParamStore, Runtime};
use wasm_bindgen::prelude::*;

use crate::texture::Texture;

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

    // Port widget callback
    {
        let pw = params::make_port_widget(&params);
        gs.port_widget.borrow_mut().replace(pw);
    }

    let runtime = Runtime::new(gs.clone(), params.clone());
    register_runtime(&runtime);

    // Custom node body callback (canvas previews watch their node's signal).
    {
        let cb = preview::make_custom_body(&runtime);
        gs.custom_node_body.borrow_mut().replace(cb);
    }

    // Body-height callback: invoked during node creation so `CustomBodyHeight`
    // is set atomically before the node is published for rendering.
    gs.body_height_for_type
        .borrow_mut()
        .replace(Rc::new(|tid: &str| {
            let h = body_height_for_type(tid);
            (h > 0.0).then_some(h)
        }));

    build_default_scene(&gs, &params);

    runtime.initial_setup();

    // Group/ungroup carry an ephemeral old→new port-ID mapping that
    // ParamStore needs to migrate its state by.
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

    runtime.spawn_reconciliation_watcher();

    dominator::append_dom(
        &dominator::body(),
        html!("div", {
            .dwclass!("w-full h-full")
            .child(render_graph_editor(gs))
        }),
    );
}

fn build_default_scene(gs: &Rc<GraphSignals>, params: &Rc<ParamStore>) {
    use nodegraph_core::graph::node::NodeHeader;

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
            (PortDirection::Input, SocketType::Int, "Seed".to_string()),
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
    let (_grass_n_id, grass_n_ports) =
        gs.add_node_typed("Noise", Some("noise"), (60.0, 40.0), noise_ports());
    params.get::<f64>(grass_n_ports[0], 5.5); // Scale (higher = finer grain)
    params.get::<i64>(grass_n_ports[1], 7); // Seed

    let (_grass_c_id, grass_c_ports) = gs.add_node_typed(
        "Colorize",
        Some("colorize"),
        (280.0, 40.0),
        colorize_ports(),
    );
    params.get::<[u8; 4]>(grass_c_ports[1], [91, 153, 56, 255]);

    // --- dirt column ---
    // Ordering: Noise → BC → Colorize. The BC step pushes noise values up so
    // the multiply in Colorize doesn't crush the dirt into near-black.
    let (_dirt_n_id, dirt_n_ports) =
        gs.add_node_typed("Noise", Some("noise"), (60.0, 280.0), noise_ports());
    params.get::<f64>(dirt_n_ports[0], 4.5); // finer pebbly dirt
    params.get::<i64>(dirt_n_ports[1], 13);

    let (_dirt_bc_id, dirt_bc_ports) = gs.add_node_typed(
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
    params.get::<f64>(dirt_bc_ports[1], 0.35); // brighten noise before tint
    params.get::<f64>(dirt_bc_ports[2], 0.4); // punch up contrast

    let (_dirt_c_id, dirt_c_ports) = gs.add_node_typed(
        "Colorize",
        Some("colorize"),
        (500.0, 280.0),
        colorize_ports(),
    );
    params.get::<[u8; 4]>(dirt_c_ports[1], [168, 120, 78, 255]);

    // --- mask column ---
    let (_grad_id, grad_ports) = gs.add_node_typed(
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
    params.get::<[u8; 4]>(grad_ports[0], [0, 0, 0, 255]); // Color A = top = black
    params.get::<[u8; 4]>(grad_ports[1], [255, 255, 255, 255]); // Color B = bottom = white

    let (_jag_n_id, jag_n_ports) =
        gs.add_node_typed("Noise", Some("noise"), (60.0, 760.0), noise_ports());
    params.get::<f64>(jag_n_ports[0], 7.0); // chunkier grass drape
    params.get::<i64>(jag_n_ports[1], 99);

    let (_mix_id, mix_ports) = gs.add_node_typed(
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
    params.get::<f64>(mix_ports[2], 0.45); // let jagged noise dominate the gradient

    let (_thresh_id, thresh_ports) = gs.add_node_typed(
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
    params.get::<f64>(thresh_ports[1], 0.3); // lower threshold → more pixels
                                             // end up above it in luminance → more dirt, less grass (Minecraft-proportion drape)

    // --- blend + preview ---
    let (_blend_id, blend_ports) = gs.add_node_typed(
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

    let (_block_preview_id, block_preview_ports) = gs.add_node_typed(
        "Block Preview",
        Some("block_preview"),
        (1080.0, 40.0),
        vec![
            (PortDirection::Input, SocketType::Image, "Top".to_string()),
            (PortDirection::Input, SocketType::Image, "Side".to_string()),
        ],
    );

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

    // ===== Grass group =====
    let (grass_noise_id, grass_noise_ports) = gs.add_node_typed(
        "Noise",
        Some("noise"),
        (100.0, 400.0),
        vec![
            (PortDirection::Input, SocketType::Float, "Scale".to_string()),
            (PortDirection::Input, SocketType::Int, "Seed".to_string()),
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

    let (_grass_color_id, grass_color_ports) = gs.add_node_typed(
        "Solid Color",
        Some("solid_color"),
        (60.0, 1040.0),
        vec![(
            PortDirection::Output,
            SocketType::Color,
            "Color".to_string(),
        )],
    );
    params.get::<[u8; 4]>(grass_color_ports[0], [74, 160, 46, 255]);

    let (_grass_tiled_id, grass_tiled_ports) = gs.add_node_typed(
        "Tiled Preview",
        Some("tiled_preview"),
        (380.0, 1040.0),
        vec![(
            PortDirection::Input,
            SocketType::Image,
            "Texture".to_string(),
        )],
    );

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
            set_subgraph_int(sub, params, "Noise", "Seed", 42);
        }
        drop(editor);
    }
}

fn set_subgraph_float(
    graph: &nodegraph_core::graph::NodeGraph,
    params: &Rc<ParamStore>,
    node_title: &str,
    port_label: &str,
    value: f64,
) {
    for_subgraph_input_port(graph, node_title, port_label, |pid| {
        params.get::<f64>(pid, value).set(value);
    });
}

fn set_subgraph_int(
    graph: &nodegraph_core::graph::NodeGraph,
    params: &Rc<ParamStore>,
    node_title: &str,
    port_label: &str,
    value: i64,
) {
    for_subgraph_input_port(graph, node_title, port_label, |pid| {
        params.get::<i64>(pid, value).set(value);
    });
}

fn for_subgraph_input_port(
    graph: &nodegraph_core::graph::NodeGraph,
    node_title: &str,
    port_label: &str,
    mut f: impl FnMut(nodegraph_core::EntityId),
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
                f(pid);
            }
        }
    }
}

/// Wire the runtime with this app's value types, primitive conversions,
/// and per-node-type computations.
#[cfg(not(test))]
fn register_runtime(runtime: &Rc<Runtime>) {
    use crate::computations;

    // Value types the app uses. Once registered, the runtime can store
    // outputs + source selectors of these types and route connections.
    runtime.register_value_type::<f64>();
    runtime.register_value_type::<i64>();
    runtime.register_value_type::<bool>();
    runtime.register_value_type::<String>();
    runtime.register_value_type::<[u8; 4]>();
    runtime.register_value_type::<Texture>();

    // Cross-type primitive conversions.
    let conv = runtime.conversions();
    conv.register::<i64, f64, _>(|i| i as f64);
    conv.register::<bool, f64, _>(|b| if b { 1.0 } else { 0.0 });
    conv.register::<f64, i64, _>(|f| f as i64);
    conv.register::<bool, i64, _>(i64::from);
    conv.register::<f64, bool, _>(|f| f != 0.0);
    conv.register::<i64, bool, _>(|i| i != 0);

    // Per-node-type computations. Const-style nodes use the library's
    // generic ConstNode<T>; all other node types have a dedicated impl
    // in `computations.rs`.
    let comps = runtime.computations();
    comps.register(
        "solid_color",
        Rc::new(ConstNode::<[u8; 4]>::new(params::default_color(
            "solid_color",
            "Color",
        ))) as Rc<dyn NodeComputation>,
    );
    comps.register(
        "const_float",
        Rc::new(ConstNode::<f64>::new(params::default_float(
            "const_float",
            "Value",
        ))) as Rc<dyn NodeComputation>,
    );
    comps.register(
        "const_int",
        Rc::new(ConstNode::<i64>::new(params::default_int(
            "const_int",
            "Value",
        ))) as Rc<dyn NodeComputation>,
    );
    comps.register(
        "const_bool",
        Rc::new(ConstNode::<bool>::new(params::default_bool(
            "const_bool",
            "Value",
        ))) as Rc<dyn NodeComputation>,
    );
    comps.register(
        "const_string",
        Rc::new(ConstNode::<String>::new(params::default_string(
            "const_string",
            "Value",
        ))) as Rc<dyn NodeComputation>,
    );

    comps.register("checker", Rc::new(computations::Checker));
    comps.register("noise", Rc::new(computations::Noise));
    comps.register("gradient", Rc::new(computations::Gradient));
    comps.register("brick", Rc::new(computations::Brick));

    comps.register("mix", Rc::new(computations::Mix));
    comps.register("blend", Rc::new(computations::Blend));
    comps.register(
        "brightness_contrast",
        Rc::new(computations::BrightnessContrast),
    );
    comps.register("threshold", Rc::new(computations::Threshold));
    comps.register("invert", Rc::new(computations::Invert));
    comps.register("colorize", Rc::new(computations::Colorize));

    // Group nodes get their own trait impl which fans inputs + internal
    // param signals into a `crate::eval::evaluate` call. Groups are
    // detected by `SubgraphRoot` component, not type_id, so they're
    // registered via `set_group_computation` instead of `register`.
    runtime.set_group_computation(Rc::new(computations::Group));
}

/// Per-type preview/output body height in pixels. `0.0` means no custom body.
/// Wired via `GraphSignals::body_height_for_type` so new nodes receive
/// `CustomBodyHeight` atomically at creation time.
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
