use wasm_bindgen::prelude::*;
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_render::graph_signals::GraphSignals;
use nodegraph_render::viewport_view::render_graph_editor;

#[wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();
    dwind::stylesheet();

    let gs = GraphSignals::new();

    // Build a demo graph
    let math_add = gs.add_node("Math Add", (50.0, 50.0), vec![
        (PortDirection::Input, SocketType::Float, "A".to_string()),
        (PortDirection::Input, SocketType::Float, "B".to_string()),
        (PortDirection::Output, SocketType::Float, "Result".to_string()),
    ]);

    let color_mix = gs.add_node("Color Mix", (300.0, 30.0), vec![
        (PortDirection::Input, SocketType::Color, "Color 1".to_string()),
        (PortDirection::Input, SocketType::Color, "Color 2".to_string()),
        (PortDirection::Input, SocketType::Float, "Factor".to_string()),
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
    ]);

    let output_node = gs.add_node("Material Output", (600.0, 60.0), vec![
        (PortDirection::Input, SocketType::Shader, "Surface".to_string()),
        (PortDirection::Input, SocketType::Shader, "Volume".to_string()),
    ]);

    let noise = gs.add_node("Noise Texture", (50.0, 250.0), vec![
        (PortDirection::Input, SocketType::Vector, "Vector".to_string()),
        (PortDirection::Input, SocketType::Float, "Scale".to_string()),
        (PortDirection::Output, SocketType::Color, "Color".to_string()),
        (PortDirection::Output, SocketType::Float, "Fac".to_string()),
    ]);

    let shader = gs.add_node("Principled BSDF", (300.0, 220.0), vec![
        (PortDirection::Input, SocketType::Color, "Base Color".to_string()),
        (PortDirection::Input, SocketType::Float, "Roughness".to_string()),
        (PortDirection::Input, SocketType::Float, "Metallic".to_string()),
        (PortDirection::Output, SocketType::Shader, "BSDF".to_string()),
    ]);

    // Connect: Math Add Result -> Color Mix Factor
    {
        let graph = gs.graph.borrow();
        let math_ports = graph.node_ports(math_add).to_vec();
        let mix_ports = graph.node_ports(color_mix).to_vec();
        let math_out = math_ports.iter().find(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
        }).copied();
        let mix_fac = mix_ports.iter().filter(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
        }).nth(2).copied(); // Factor is the 3rd input
        drop(graph);
        if let (Some(src), Some(tgt)) = (math_out, mix_fac) {
            gs.connect_ports(src, tgt);
        }
    }

    // Connect: Noise Color -> Principled Base Color
    {
        let graph = gs.graph.borrow();
        let noise_ports = graph.node_ports(noise).to_vec();
        let shader_ports = graph.node_ports(shader).to_vec();
        let noise_color = noise_ports.iter().filter(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
        }).nth(0).copied();
        let shader_base = shader_ports.iter().filter(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
        }).nth(0).copied();
        drop(graph);
        if let (Some(src), Some(tgt)) = (noise_color, shader_base) {
            gs.connect_ports(src, tgt);
        }
    }

    // Connect: Principled BSDF -> Material Output Surface
    {
        let graph = gs.graph.borrow();
        let shader_ports = graph.node_ports(shader).to_vec();
        let out_ports = graph.node_ports(output_node).to_vec();
        let bsdf = shader_ports.iter().find(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
        }).copied();
        let surface = out_ports.iter().find(|&&p| {
            graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
        }).copied();
        drop(graph);
        if let (Some(src), Some(tgt)) = (bsdf, surface) {
            gs.connect_ports(src, tgt);
        }
    }

    dominator::append_dom(&dominator::body(), render_graph_editor(gs));
}
