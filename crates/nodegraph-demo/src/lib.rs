use wasm_bindgen::prelude::*;
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::search::{NodeTypeDefinition, PortDefinition, NodeTypeRegistry};
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_render::graph_signals::GraphSignals;
use nodegraph_render::viewport_view::render_graph_editor;

fn register_demo_node_types(reg: &mut NodeTypeRegistry) {
    reg.register(NodeTypeDefinition {
        type_id: "math_add".into(), display_name: "Math Add".into(), category: "Math".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "A".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "B".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Result".into() },
        ],
    });
    reg.register(NodeTypeDefinition {
        type_id: "color_mix".into(), display_name: "Color Mix".into(), category: "Color".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Color 1".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Color 2".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Factor".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Color, label: "Color".into() },
        ],
    });
    reg.register(NodeTypeDefinition {
        type_id: "noise_texture".into(), display_name: "Noise Texture".into(), category: "Texture".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Vector, label: "Vector".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Scale".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Color, label: "Color".into() },
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Fac".into() },
        ],
    });
    reg.register(NodeTypeDefinition {
        type_id: "principled_bsdf".into(), display_name: "Principled BSDF".into(), category: "Shader".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Base Color".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Roughness".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Metallic".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Shader, label: "BSDF".into() },
        ],
    });
    reg.register(NodeTypeDefinition {
        type_id: "material_output".into(), display_name: "Material Output".into(), category: "Output".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Shader, label: "Surface".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Shader, label: "Volume".into() },
        ],
        output_ports: vec![],
    });
    reg.register(NodeTypeDefinition {
        type_id: "reroute".into(), display_name: "Reroute".into(), category: "Utility".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into() },
        ],
    });
    // Group IO nodes — only functional inside subgraphs
    reg.register(NodeTypeDefinition {
        type_id: "group_input".into(), display_name: "Group Input".into(), category: "Group".into(),
        input_ports: vec![], output_ports: vec![],
    });
    reg.register(NodeTypeDefinition {
        type_id: "group_output".into(), display_name: "Group Output".into(), category: "Group".into(),
        input_ports: vec![], output_ports: vec![],
    });
}

#[wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();
    dwind::stylesheet();

    let gs = GraphSignals::new();

    // Register node types for the search menu
    register_demo_node_types(&mut gs.registry.borrow_mut());

    // Build a demo graph
    let (math_add, _) = gs.add_node("Math Add", (50.0, 50.0), vec![
        (PortDirection::Input, SocketType::Float, "A".to_string()),
        (PortDirection::Input, SocketType::Float, "B".to_string()),
        (PortDirection::Output, SocketType::Float, "Result".to_string()),
    ]);

    let (color_mix, _) = gs.add_node("Color Mix", (300.0, 30.0), vec![
        (PortDirection::Input, SocketType::Color, "Color 1".to_string()),
        (PortDirection::Input, SocketType::Color, "Color 2".to_string()),
        (PortDirection::Input, SocketType::Float, "Factor".to_string()),
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
    ]);

    let (output_node, _) = gs.add_node("Material Output", (600.0, 60.0), vec![
        (PortDirection::Input, SocketType::Shader, "Surface".to_string()),
        (PortDirection::Input, SocketType::Shader, "Volume".to_string()),
    ]);

    let (noise, _) = gs.add_node("Noise Texture", (50.0, 250.0), vec![
        (PortDirection::Input, SocketType::Vector, "Vector".to_string()),
        (PortDirection::Input, SocketType::Float, "Scale".to_string()),
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
        (PortDirection::Output, SocketType::Float, "Fac".to_string()),
    ]);

    let (shader, _) = gs.add_node("Principled BSDF", (300.0, 220.0), vec![
        (PortDirection::Input, SocketType::Color, "Base Color".to_string()),
        (PortDirection::Input, SocketType::Float, "Roughness".to_string()),
        (PortDirection::Input, SocketType::Float, "Metallic".to_string()),
        (PortDirection::Output, SocketType::Shader, "BSDF".to_string()),
    ]);

    // Connect: Math Add Result -> Color Mix Factor
    {
        let editor = gs.editor.borrow();
        let graph = editor.current_graph();
        let math_ports = graph.node_ports(math_add).to_vec();
        let mix_ports = graph.node_ports(color_mix).to_vec();
        let math_out = math_ports.iter().find(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
        }).copied();
        let mix_fac = mix_ports.iter().filter(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
        }).nth(2).copied(); // Factor is the 3rd input
        drop(editor);
        if let (Some(src), Some(tgt)) = (math_out, mix_fac) {
            gs.connect_ports(src, tgt).unwrap();
        }
    }

    // Connect: Noise Color -> Principled Base Color
    {
        let editor = gs.editor.borrow();
        let graph = editor.current_graph();
        let noise_ports = graph.node_ports(noise).to_vec();
        let shader_ports = graph.node_ports(shader).to_vec();
        let noise_color = noise_ports.iter().filter(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
        }).nth(0).copied();
        let shader_base = shader_ports.iter().filter(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
        }).nth(0).copied();
        drop(editor);
        if let (Some(src), Some(tgt)) = (noise_color, shader_base) {
            gs.connect_ports(src, tgt).unwrap();
        }
    }

    // Connect: Principled BSDF -> Material Output Surface
    {
        let editor = gs.editor.borrow();
        let graph = editor.current_graph();
        let shader_ports = graph.node_ports(shader).to_vec();
        let out_ports = graph.node_ports(output_node).to_vec();
        let bsdf = shader_ports.iter().find(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
        }).copied();
        let surface = out_ports.iter().find(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
        }).copied();
        drop(editor);
        if let (Some(src), Some(tgt)) = (bsdf, surface) {
            gs.connect_ports(src, tgt).unwrap();
        }
    }

    dominator::append_dom(&dominator::body(), render_graph_editor(gs));
}
