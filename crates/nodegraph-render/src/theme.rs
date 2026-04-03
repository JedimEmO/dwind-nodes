use std::rc::Rc;

pub struct Theme {
    // Canvas
    pub canvas_bg: &'static str,

    // Node
    pub node_bg: &'static str,
    pub group_node_bg: &'static str,
    pub group_node_border: &'static str,
    pub selection_highlight: &'static str,
    pub header_text: &'static str,
    pub port_label_text: &'static str,

    // Reroute
    pub reroute_fill: &'static str,
    pub reroute_stroke: &'static str,

    // Group IO nodes
    pub io_node_input_bg: &'static str,
    pub io_node_output_bg: &'static str,
    pub io_node_text: &'static str,

    // Ports
    pub port_stroke: &'static str,

    // Connections
    pub cut_line: &'static str,
    pub preview_wire: &'static str,

    // Search menu
    pub menu_bg: &'static str,
    pub menu_border: &'static str,
    pub menu_input_border: &'static str,
    pub menu_text: &'static str,
    pub menu_input_bg: &'static str,
    pub menu_input_text: &'static str,
    pub menu_selected_bg: &'static str,
    pub menu_selected_border: &'static str,
    pub menu_category_text: &'static str,
    pub menu_shadow: &'static str,

    // Breadcrumb
    pub breadcrumb_text: &'static str,
    pub breadcrumb_bg: &'static str,

    // Box select
    pub box_select_border: &'static str,
    pub box_select_fill: &'static str,

    // Frame opacity
    pub frame_fill_opacity: f64,
    pub frame_stroke_opacity: f64,
    pub frame_selected_opacity: f64,

    // Minimap
    pub minimap_bg: &'static str,
    pub minimap_border: &'static str,
    pub minimap_viewport_fill: &'static str,
    pub minimap_viewport_stroke: &'static str,
    pub minimap_connection: &'static str,
}

impl Theme {
    pub fn dark() -> Rc<Self> {
        Rc::new(Self {
            // Canvas
            canvas_bg: "#1a1a2e",

            // Node
            node_bg: "#2d2d3d",
            group_node_bg: "#2d3d2d",
            group_node_border: "#4a7a4a",
            selection_highlight: "#4a9eff",
            header_text: "white",
            port_label_text: "#ccc",

            // Reroute
            reroute_fill: "#444",
            reroute_stroke: "#888",

            // Group IO nodes
            io_node_input_bg: "#2d3d4d",
            io_node_output_bg: "#3d2d4d",
            io_node_text: "#ccc",

            // Ports
            port_stroke: "rgba(255,255,255,0.3)",

            // Connections
            cut_line: "#ff4444",
            preview_wire: "#4a9eff",

            // Search menu
            menu_bg: "#2a2a3e",
            menu_border: "#555",
            menu_input_border: "#444",
            menu_text: "#ccc",
            menu_input_bg: "#1e1e30",
            menu_input_text: "white",
            menu_selected_bg: "#3a3a5e",
            menu_selected_border: "#4a9eff",
            menu_category_text: "#888",
            menu_shadow: "0 4px 16px rgba(0,0,0,0.5)",

            // Breadcrumb
            breadcrumb_text: "#aaa",
            breadcrumb_bg: "rgba(30,30,48,0.8)",

            // Box select
            box_select_border: "#4a9eff",
            box_select_fill: "rgba(74, 158, 255, 0.1)",

            // Frame
            frame_fill_opacity: 0.15,
            frame_stroke_opacity: 0.4,
            frame_selected_opacity: 0.9,

            // Minimap
            minimap_bg: "rgba(26, 26, 46, 0.9)",
            minimap_border: "#444",
            minimap_viewport_fill: "rgba(74, 158, 255, 0.15)",
            minimap_viewport_stroke: "rgba(74, 158, 255, 0.6)",
            minimap_connection: "#555",
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        Rc::try_unwrap(Self::dark()).unwrap_or_else(|rc| (*rc).clone())
    }
}

impl Clone for Theme {
    fn clone(&self) -> Self {
        // All fields are Copy (&'static str and f64)
        Self { ..*self }
    }
}
