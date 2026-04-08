use nodegraph_core::{NodeTypeDefinition, PortDefinition, PortDirection, SocketType};
use nodegraph_core::search::NodeTypeRegistry;

pub fn register_all(reg: &mut NodeTypeRegistry) {
    // === Generators ===

    reg.register(NodeTypeDefinition {
        type_id: "solid_color".into(),
        display_name: "Solid Color".into(),
        category: "Generator".into(),
        input_ports: vec![],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Color, label: "Color".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "checker".into(),
        display_name: "Checker".into(),
        category: "Generator".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Color A".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Color B".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Size".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "noise".into(),
        display_name: "Noise".into(),
        category: "Generator".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Scale".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Seed".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "gradient".into(),
        display_name: "Gradient".into(),
        category: "Generator".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Color A".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Color B".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "brick".into(),
        display_name: "Brick".into(),
        category: "Generator".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Mortar".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Brick".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Rows".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    // === Filters ===

    reg.register(NodeTypeDefinition {
        type_id: "mix".into(),
        display_name: "Mix".into(),
        category: "Filter".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "A".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "B".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Factor".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "brightness_contrast".into(),
        display_name: "Brightness/Contrast".into(),
        category: "Filter".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Brightness".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Contrast".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "threshold".into(),
        display_name: "Threshold".into(),
        category: "Filter".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Level".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "invert".into(),
        display_name: "Invert".into(),
        category: "Filter".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "colorize".into(),
        display_name: "Colorize".into(),
        category: "Filter".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Tint".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Image, label: "Texture".into() },
        ],
    });

    // === Output ===

    reg.register(NodeTypeDefinition {
        type_id: "preview".into(),
        display_name: "Preview".into(),
        category: "Output".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
        ],
        output_ports: vec![],
    });

    reg.register(NodeTypeDefinition {
        type_id: "tiled_preview".into(),
        display_name: "Tiled Preview".into(),
        category: "Output".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
        ],
        output_ports: vec![],
    });

    reg.register(NodeTypeDefinition {
        type_id: "iso_preview".into(),
        display_name: "Iso Preview".into(),
        category: "Output".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Image, label: "Texture".into() },
        ],
        output_ports: vec![],
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;


    fn make_registry() -> NodeTypeRegistry {
        let mut reg = NodeTypeRegistry::new();
        register_all(&mut reg);
        reg
    }

    #[wasm_bindgen_test]
    fn register_all_thirteen_types() {
        let reg = make_registry();
        // 5 generators + 5 filters + 3 outputs = 13
        assert_eq!(reg.all().len(), 13);
    }

    #[wasm_bindgen_test]
    fn checker_ports_correct() {
        let reg = make_registry();
        let checker = reg.get("checker").expect("checker type not found");

        assert_eq!(checker.input_ports.len(), 3);

        assert_eq!(checker.input_ports[0].label, "Color A");
        assert_eq!(checker.input_ports[0].socket_type, SocketType::Color);

        assert_eq!(checker.input_ports[1].label, "Color B");
        assert_eq!(checker.input_ports[1].socket_type, SocketType::Color);

        assert_eq!(checker.input_ports[2].label, "Size");
        assert_eq!(checker.input_ports[2].socket_type, SocketType::Float);

        assert_eq!(checker.output_ports.len(), 1);
        assert_eq!(checker.output_ports[0].label, "Texture");
        assert_eq!(checker.output_ports[0].socket_type, SocketType::Image);
    }

    #[wasm_bindgen_test]
    fn output_nodes_no_outputs() {
        let reg = make_registry();

        for type_id in &["preview", "tiled_preview", "iso_preview"] {
            let def = reg.get(type_id).unwrap_or_else(|| panic!("{} type not found", type_id));
            assert!(
                def.output_ports.is_empty(),
                "{} should have no output ports but has {}",
                type_id,
                def.output_ports.len()
            );
        }
    }
}
