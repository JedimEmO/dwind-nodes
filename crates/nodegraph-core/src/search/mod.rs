use crate::graph::port::PortDirection;
use crate::types::socket_type::SocketType;

#[derive(Clone, Debug)]
pub struct PortDefinition {
    pub direction: PortDirection,
    pub socket_type: SocketType,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct NodeTypeDefinition {
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    pub input_ports: Vec<PortDefinition>,
    pub output_ports: Vec<PortDefinition>,
}

pub struct NodeTypeRegistry {
    types: Vec<NodeTypeDefinition>,
}

impl NodeTypeRegistry {
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }

    pub fn register(&mut self, def: NodeTypeDefinition) {
        self.types.push(def);
    }

    pub fn all(&self) -> &[NodeTypeDefinition] {
        &self.types
    }

    pub fn get(&self, type_id: &str) -> Option<&NodeTypeDefinition> {
        self.types.iter().find(|t| t.type_id == type_id)
    }

    /// Search by display_name and category (case-insensitive substring match).
    pub fn search(&self, query: &str) -> Vec<&NodeTypeDefinition> {
        if query.is_empty() {
            return self.types.iter().collect();
        }
        let q = query.to_lowercase();
        self.types.iter().filter(|t| {
            t.display_name.to_lowercase().contains(&q) ||
            t.category.to_lowercase().contains(&q)
        }).collect()
    }

    /// Filter to node types that have at least one port compatible with the given
    /// source (for noodle-drop-to-add). `from_output` means we need an input port.
    pub fn search_compatible(&self, query: &str, src_type: SocketType, from_output: bool) -> Vec<&NodeTypeDefinition> {
        self.search(query).into_iter().filter(|t| {
            let target_ports = if from_output { &t.input_ports } else { &t.output_ports };
            target_ports.iter().any(|p| src_type.is_compatible_with(&p.socket_type))
        }).collect()
    }
}

impl Default for NodeTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> NodeTypeRegistry {
        let mut reg = NodeTypeRegistry::new();
        reg.register(NodeTypeDefinition {
            type_id: "math_add".into(),
            display_name: "Math Add".into(),
            category: "Math".into(),
            input_ports: vec![
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "A".into() },
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "B".into() },
            ],
            output_ports: vec![
                PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Result".into() },
            ],
        });
        reg.register(NodeTypeDefinition {
            type_id: "color_mix".into(),
            display_name: "Color Mix".into(),
            category: "Color".into(),
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
            type_id: "principled_bsdf".into(),
            display_name: "Principled BSDF".into(),
            category: "Shader".into(),
            input_ports: vec![
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Color, label: "Base Color".into() },
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Roughness".into() },
            ],
            output_ports: vec![
                PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Shader, label: "BSDF".into() },
            ],
        });
        reg
    }

    #[test]
    fn search_empty_returns_all() {
        let reg = make_registry();
        assert_eq!(reg.search("").len(), 3);
    }

    #[test]
    fn search_by_name() {
        let reg = make_registry();
        let results = reg.search("math");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].type_id, "math_add");
    }

    #[test]
    fn search_by_category() {
        let reg = make_registry();
        let results = reg.search("shader");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].type_id, "principled_bsdf");
    }

    #[test]
    fn search_case_insensitive() {
        let reg = make_registry();
        assert_eq!(reg.search("MATH").len(), 1);
        assert_eq!(reg.search("Color").len(), 1); // matches "Color Mix" name and "Color" category
    }

    #[test]
    fn get_by_type_id() {
        let reg = make_registry();
        assert!(reg.get("math_add").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn search_compatible_from_float_output() {
        let reg = make_registry();
        // Dragging from a Float output — need nodes with Float-compatible inputs
        let results = reg.search_compatible("", SocketType::Float, true);
        // Math Add (Float inputs), Color Mix (Float Factor input), Principled (Float Roughness)
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn search_compatible_from_shader_output() {
        let reg = make_registry();
        // Dragging from a Shader output — need nodes with Shader-compatible inputs
        let results = reg.search_compatible("", SocketType::Shader, true);
        // None of our test nodes have Shader inputs
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn search_compatible_from_color_input() {
        let reg = make_registry();
        // Dragging from a Color input — need nodes with Color-compatible outputs
        let results = reg.search_compatible("", SocketType::Color, false);
        // Color Mix (Color output) + Math Add (Float output, Float↔Color conversion)
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn any_type_compatible_with_everything() {
        assert!(SocketType::Any.is_compatible_with(&SocketType::Float));
        assert!(SocketType::Any.is_compatible_with(&SocketType::Shader));
        assert!(SocketType::Float.is_compatible_with(&SocketType::Any));
        assert!(SocketType::Any.is_compatible_with(&SocketType::Any));
    }
}
