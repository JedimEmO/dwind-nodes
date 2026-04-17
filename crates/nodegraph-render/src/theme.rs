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
    /// Dark theme. Colors are drawn from the dwind 0.6 palette so that SVG
    /// `.attr()` consumers and HTML `dwclass!` consumers stay in visual sync.
    /// The mapping is approximate — dwind's palettes don't include the exact
    /// hex values the theme used before, so some hues have shifted slightly.
    pub fn dark() -> Rc<Self> {
        Rc::new(Self {
            // Canvas — bunker-900 (#1C1C25)
            canvas_bg: "#1C1C25",

            // Node — woodsmoke-800 (#29292F)
            node_bg: "#29292F",
            // Group node — slight green tint from green-950 (#052E16)
            group_node_bg: "#052E16",
            group_node_border: "#15803D",   // green-700
            selection_highlight: "#5FB0D5", // picton-blue-400
            header_text: "#FFFFFF",         // white-50
            port_label_text: "#D1D5DB",     // gray-300

            // Reroute
            reroute_fill: "#374151",   // gray-700
            reroute_stroke: "#6B7280", // gray-500

            // Group IO nodes
            io_node_input_bg: "#082937",  // picton-blue-900
            io_node_output_bg: "#210837", // purple-900
            io_node_text: "#D1D5DB",      // gray-300

            // Ports (alpha required — palette has no transparent entries)
            port_stroke: "rgba(255,255,255,0.3)",

            // Connections
            cut_line: "#D55F5F",     // red-400
            preview_wire: "#5FB0D5", // picton-blue-400

            // Search menu
            menu_bg: "#333340",              // bunker-800
            menu_border: "#4B5563",          // gray-600
            menu_input_border: "#374151",    // gray-700
            menu_text: "#D1D5DB",            // gray-300
            menu_input_bg: "#1C1C25",        // bunker-900
            menu_input_text: "#FFFFFF",      // white-50
            menu_selected_bg: "#52525F",     // bunker-700
            menu_selected_border: "#5FB0D5", // picton-blue-400
            menu_category_text: "#6B7280",   // gray-500
            menu_shadow: "0 4px 16px rgba(0,0,0,0.5)",

            // Breadcrumb
            breadcrumb_text: "#9CA3AF",          // gray-400
            breadcrumb_bg: "rgba(28,28,37,0.8)", // bunker-900 @ 80%

            // Box select — picton-blue-400 with alpha for fill
            box_select_border: "#5FB0D5",
            box_select_fill: "rgba(95, 176, 213, 0.1)",

            // Frame
            frame_fill_opacity: 0.15,
            frame_stroke_opacity: 0.4,
            frame_selected_opacity: 0.9,

            // Minimap
            minimap_bg: "rgba(28, 28, 37, 0.9)", // bunker-900 @ 90%
            minimap_border: "#374151",           // gray-700
            minimap_viewport_fill: "rgba(95, 176, 213, 0.15)",
            minimap_viewport_stroke: "rgba(95, 176, 213, 0.6)",
            minimap_connection: "#4B5563", // gray-600
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
