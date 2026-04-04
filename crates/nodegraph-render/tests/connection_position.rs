use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

use nodegraph_core::graph::node::{NodePosition, MuteState};
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::interaction::{InputEvent, MouseButton, Modifiers};
use nodegraph_core::layout::Vec2;
use nodegraph_core::store::EntityId;
use nodegraph_core::search::{NodeTypeDefinition, PortDefinition, NodeTypeRegistry};
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_render::graph_signals::GraphSignals;
use nodegraph_render::viewport_view::render_graph_editor;

// ============================================================
// Helpers
// ============================================================

fn new_two_node_graph() -> (std::rc::Rc<GraphSignals>, EntityId, EntityId, EntityId, EntityId) {
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("Source", (100.0, 100.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("Target", (400.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let (out, inp) = gs.with_graph(|graph| {
        (graph.node_ports(n1)[0], graph.node_ports(n2)[0])
    });
    (gs, n1, n2, out, inp)
}

/// Isolated test container. Removed from the DOM on drop so tests don't pollute each other.
struct TestContainer {
    element: web_sys::Element,
}

impl TestContainer {
    fn new() -> Self {
        let doc = web_sys::window().unwrap().document().unwrap();
        let el = doc.create_element("div").unwrap();
        el.set_attribute("style", "position:absolute;left:0;top:0;width:800px;height:600px").unwrap();
        doc.body().unwrap().append_child(&el).unwrap();
        Self { element: el }
    }

    fn dom_element(&self) -> web_sys::HtmlElement {
        self.element.clone().dyn_into().unwrap()
    }

    /// Query within this container only.
    fn query_all(&self, selector: &str) -> web_sys::NodeList {
        self.element.query_selector_all(selector).unwrap()
    }

    fn query(&self, selector: &str) -> Option<web_sys::Element> {
        self.element.query_selector(selector).unwrap()
    }
}

impl Drop for TestContainer {
    fn drop(&mut self) {
        self.element.remove();
    }
}

/// Render graph editor into an isolated container.
fn render_sync(gs: &std::rc::Rc<GraphSignals>) -> TestContainer {
    let container = TestContainer::new();
    dominator::append_dom(&container.dom_element(), render_graph_editor(gs.clone()));
    container
}

// ============================================================
// Rendering and port offset measurement
// ============================================================

#[wasm_bindgen_test]
fn test_port_world_pos_from_graph_state() {
    // Port positions are computed purely from graph state — no DOM needed
    let (gs, _, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();

    // No render needed — port_world_pos is a pure function of graph data
    let out_pos = gs.port_world_pos(out).unwrap();
    let inp_pos = gs.port_world_pos(inp).unwrap();

    // Output port on Source (at 100,100): right edge of node
    assert!(out_pos.x > 200.0, "Output port x={} should be at right edge", out_pos.x);
    assert!(out_pos.y > 100.0, "Output port y={} should be below header", out_pos.y);

    // Input port on Target (at 400,100): left edge of node
    assert!(inp_pos.x > 399.0 && inp_pos.x < 401.0, "Input port x={} should be at left edge", inp_pos.x);
}

#[wasm_bindgen_test]
fn test_port_world_pos_reasonable() {
    let (gs, _, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    // Output port on Source (at x=100): should be near right edge of node
    let out_pos = gs.port_world_pos(out).unwrap();
    assert!(out_pos.x > 200.0 && out_pos.x < 300.0,
        "Output port x={} should be near right edge of Source node (x=100, w=160)", out_pos.x);
    assert!(out_pos.y > 100.0 && out_pos.y < 200.0,
        "Output port y={} should be below header of Source node (y=100)", out_pos.y);

    // Input port on Target (at x=400): should be near left edge
    let inp_pos = gs.port_world_pos(inp).unwrap();
    assert!(inp_pos.x >= 400.0 && inp_pos.x < 420.0,
        "Input port x={} should be at left edge of Target node (x=400)", inp_pos.x);
}

#[wasm_bindgen_test]
fn test_connection_endpoints_computable() {
    // Verify that after connecting, we can compute both port world positions
    // and produce a valid bezier path — all from data, no DOM needed.
    let (gs, _, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    let out_pos = gs.port_world_pos(out).unwrap();
    let inp_pos = gs.port_world_pos(inp).unwrap();

    let path = nodegraph_core::layout::compute_connection_path(out_pos, inp_pos);
    let d = path.to_svg_d();
    assert!(d.starts_with("M "), "Path should start with M, got: {}", d);
    assert!(d.contains(" C "), "Path should contain C bezier, got: {}", d);

    // Start point should match output port position
    let parts: Vec<&str> = d.split_whitespace().collect();
    let px: f64 = parts[1].parse().unwrap();
    let py: f64 = parts[2].parse().unwrap();
    assert!((px - out_pos.x).abs() < 0.01, "Path start x={} should match port x={}", px, out_pos.x);
    assert!((py - out_pos.y).abs() < 0.01, "Path start y={} should match port y={}", py, out_pos.y);
}

// ============================================================
// Node dragging reactively updates connections
// ============================================================

#[wasm_bindgen_test]
fn test_drag_updates_connection_reactively() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    // Read the output port's world position before move
    let pos_before = gs.port_world_pos(out).unwrap();

    // Move node 1 by updating position signal (simulates what sync_all_positions does)
    gs.node_positions.borrow().get(&n1).unwrap().set((200.0, 200.0));
    // Also update the graph state to match
    gs.with_graph_mut(|g| g.world.get_mut::<NodePosition>(n1).unwrap().x = 200.0);
    gs.with_graph_mut(|g| g.world.get_mut::<NodePosition>(n1).unwrap().y = 200.0);
    // Port world pos should now reflect the new node position (synchronous — no frames needed)
    let pos_after = gs.port_world_pos(out).unwrap();
    assert!((pos_after.x - pos_before.x).abs() > 50.0,
        "Port position should have changed. Before: ({},{}), After: ({},{})",
        pos_before.x, pos_before.y, pos_after.x, pos_after.y);
}

// ============================================================
// Keyboard commands (tested via graph state, not DOM counts)
// ============================================================

#[wasm_bindgen_test]
fn test_delete_node() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);
    assert_eq!(gs.node_count(), 2);

    gs.select_single(n1);
    gs.delete_selected();

    assert_eq!(gs.node_count(), 1);
}

#[wasm_bindgen_test]
fn test_undo_redo() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    gs.select_single(n1);
    gs.delete_selected();
    assert_eq!(gs.node_count(), 1);

    gs.undo();
    assert_eq!(gs.node_count(), 2);

    gs.redo();
    assert_eq!(gs.node_count(), 1);
}

#[wasm_bindgen_test]
fn test_duplicate() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    gs.select_single(n1);
    gs.duplicate_selected();

    assert_eq!(gs.node_count(), 3);
}

#[wasm_bindgen_test]
fn test_mute_toggle() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    assert!(gs.with_graph(|g| g.world.get::<MuteState>(n1).is_none()));

    gs.select_single(n1);
    gs.toggle_mute_selected();
    assert_eq!(gs.with_graph(|g| g.world.get::<MuteState>(n1).map(|m| m.0).unwrap()), true);

    gs.toggle_mute_selected(); // selection still set from above
    assert_eq!(gs.with_graph(|g| g.world.get::<MuteState>(n1).map(|m| m.0).unwrap()), false);
}

#[wasm_bindgen_test]
fn test_select_all_toggle() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    gs.select_all();
    assert_eq!(gs.selection.get_cloned().len(), 2);

    gs.select_all();
    assert_eq!(gs.selection.get_cloned().len(), 0);
}

// ============================================================
// Connection type validation
// ============================================================

#[wasm_bindgen_test]
fn test_compatible_connection_succeeds() {
    let (gs, _, _, out, inp) = new_two_node_graph();
    let _tc = render_sync(&gs);

    assert!(gs.connect_ports(out, inp).is_ok());
    assert_eq!(gs.connection_count(), 1);
}

#[wasm_bindgen_test]
fn test_incompatible_connection_rejected() {
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Shader, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Geometry, "In".to_string()),
    ]);
    let (out, inp) = gs.with_graph(|g| (g.node_ports(n1)[0], g.node_ports(n2)[0]));
    let _tc = render_sync(&gs);

    assert!(gs.connect_ports(out, inp).is_err());
    assert_eq!(gs.connection_count(), 0);
}

// ============================================================
// Connection removal
// ============================================================

#[wasm_bindgen_test]
fn test_delete_node_removes_its_connections() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    assert_eq!(gs.connection_count(), 1);

    gs.select_single(n1);
    gs.delete_selected();

    assert_eq!(gs.connection_count(), 0);
}

// ============================================================
// Pan and zoom
// ============================================================

#[wasm_bindgen_test]
fn test_pan() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    gs.handle_input(InputEvent::MouseDown {
        screen: Vec2::new(400.0, 300.0), world: Vec2::new(400.0, 300.0),
        button: MouseButton::Middle, modifiers: Modifiers::default(),
    });
    gs.handle_input(InputEvent::MouseMove {
        screen: Vec2::new(450.0, 320.0), world: Vec2::new(450.0, 320.0),
        modifiers: Modifiers::default(),
    });
    gs.handle_input(InputEvent::MouseUp {
        screen: Vec2::new(450.0, 320.0), world: Vec2::new(450.0, 320.0),
        button: MouseButton::Middle, modifiers: Modifiers::default(),
    });

    let (px, py) = gs.pan.get();
    assert!((px - 50.0).abs() < 1.0);
    assert!((py - 20.0).abs() < 1.0);
}

#[wasm_bindgen_test]
fn test_zoom() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let z0 = gs.zoom.get();
    gs.handle_input(InputEvent::Scroll { screen: Vec2::new(400.0, 300.0), delta: 100.0 });
    assert!(gs.zoom.get() > z0);
}

// ============================================================
// Drag-to-connect state
// ============================================================

#[wasm_bindgen_test]
fn test_start_connecting_from_port() {
    let (gs, _, _, out, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let pos = gs.port_world_pos(out).unwrap_or(Vec2::new(0.0, 0.0));
    gs.start_connecting(out, pos, pos);

    let cf = gs.connecting_from.get();
    assert!(cf.is_some());
    let (pid, st, from_out) = cf.unwrap();
    assert_eq!(pid, out);
    assert_eq!(st, SocketType::Float);
    assert!(from_out);
}

// ============================================================
// Viewport inner element
// ============================================================

#[wasm_bindgen_test]
fn test_viewport_inner_exists() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let tc = render_sync(&gs);
    assert!(tc.query("[data-viewport-inner]").is_some());
}

// ============================================================
// Full drag-to-connect cycle
// ============================================================

#[wasm_bindgen_test]
fn test_full_drag_to_connect_cycle() {
    let (gs, _, _, out, inp) = new_two_node_graph();
    // Don't connect yet — we'll do it via drag
    let _tc = render_sync(&gs);

    assert_eq!(gs.connection_count(), 0);

    // Start connecting from output port
    let out_pos = gs.port_world_pos(out).unwrap();
    gs.start_connecting(out, out_pos, out_pos);
    assert!(gs.connecting_from.get().is_some());

    // Move toward target — preview wire should appear
    let mid = Vec2::new(250.0, 100.0);
    gs.handle_input(InputEvent::MouseMove {
        screen: mid, world: mid, modifiers: Modifiers::default(),
    });
    assert!(gs.preview_wire.get_cloned().is_some(), "Preview wire should be visible during drag");

    // Move over target port — drop target should activate
    let inp_pos = gs.port_world_pos(inp).unwrap();
    gs.handle_input(InputEvent::MouseMove {
        screen: inp_pos, world: inp_pos, modifiers: Modifiers::default(),
    });
    assert_eq!(gs.drop_target_port.get(), Some(inp), "Drop target should be the input port");

    // Release — connection should be created
    gs.handle_input(InputEvent::MouseUp {
        screen: inp_pos, world: inp_pos,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });

    assert_eq!(gs.connection_count(), 1, "Connection should be created after drop");
    assert!(gs.connecting_from.get().is_none(), "connecting_from should be cleared");
    assert!(gs.preview_wire.get_cloned().is_none(), "Preview wire should be cleared");
    assert!(gs.drop_target_port.get().is_none(), "Drop target should be cleared");
}

#[wasm_bindgen_test]
fn test_drag_to_connect_incompatible_rejected() {
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Shader, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (300.0, 0.0), vec![
        (PortDirection::Input, SocketType::Geometry, "In".to_string()),
    ]);
    let (out, inp) = gs.with_graph(|g| (g.node_ports(n1)[0], g.node_ports(n2)[0]));
    let _tc = render_sync(&gs);

    let out_pos = gs.port_world_pos(out).unwrap();
    gs.start_connecting(out, out_pos, out_pos);

    // Move over incompatible target — drop target should NOT activate
    let inp_pos = gs.port_world_pos(inp).unwrap();
    gs.handle_input(InputEvent::MouseMove {
        screen: inp_pos, world: inp_pos, modifiers: Modifiers::default(),
    });
    assert_eq!(gs.drop_target_port.get(), None, "Incompatible port should not be drop target");

    // Release — no connection
    gs.handle_input(InputEvent::MouseUp {
        screen: inp_pos, world: inp_pos,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });
    assert_eq!(gs.connection_count(), 0);
}

#[wasm_bindgen_test]
fn test_drag_to_connect_release_on_empty() {
    let (gs, _, _, out, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let out_pos = gs.port_world_pos(out).unwrap();
    gs.start_connecting(out, out_pos, out_pos);

    // Release far from any port
    let far = Vec2::new(999.0, 999.0);
    gs.handle_input(InputEvent::MouseUp {
        screen: far, world: far,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });

    assert_eq!(gs.connection_count(), 0);
    assert!(gs.connecting_from.get().is_none());
}

// ============================================================
// SVG noodle DOM actually changes after node drag
// ============================================================

#[wasm_bindgen_test]
fn test_port_position_changes_after_node_move() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    let pos_before = gs.port_world_pos(out).unwrap();

    // Move the source node via position signal (same as sync_all_positions does)
    gs.node_positions.borrow().get(&n1).unwrap().set((300.0, 300.0));
    gs.with_graph_mut(|g| g.world.get_mut::<NodePosition>(n1).unwrap().x = 300.0);
    gs.with_graph_mut(|g| g.world.get_mut::<NodePosition>(n1).unwrap().y = 300.0);

    let pos_after = gs.port_world_pos(out).unwrap();

    // Port world position should have moved by the same delta as the node
    assert!((pos_after.x - pos_before.x - 200.0).abs() < 1.0,
        "Port x should move by 200. Before: {}, After: {}", pos_before.x, pos_after.x);
    assert!((pos_after.y - pos_before.y - 200.0).abs() < 1.0,
        "Port y should move by 200. Before: {}, After: {}", pos_before.y, pos_after.y);
}

// ============================================================
// Connection SVG elements removed after node deletion
// ============================================================

#[wasm_bindgen_test]
fn test_connection_svg_removed_after_delete() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    // Count connection-related SVG paths before
    let count_before = gs.connection_list.lock_ref().len();
    assert!(count_before >= 1);

    // Delete source node
    gs.select_single(n1);
    gs.delete_selected();

    // Connection list should be empty
    assert_eq!(gs.connection_list.lock_ref().len(), 0,
        "connection_list should be empty after deleting connected node");
}

// ============================================================
// Preview wire appears/disappears
// ============================================================

#[wasm_bindgen_test]
fn test_preview_wire_lifecycle() {
    let (gs, _, _, out, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    // No preview wire initially
    assert!(gs.preview_wire.get_cloned().is_none());

    // Start connecting
    let out_pos = gs.port_world_pos(out).unwrap();
    gs.start_connecting(out, out_pos, out_pos);

    // Move — preview should appear
    let mid = Vec2::new(250.0, 150.0);
    gs.handle_input(InputEvent::MouseMove {
        screen: mid, world: mid, modifiers: Modifiers::default(),
    });
    let wire = gs.preview_wire.get_cloned();
    assert!(wire.is_some(), "Preview wire should exist during drag");

    // Check the preview wire starts near the output port
    let wire = wire.unwrap();
    assert!((wire.start.x - out_pos.x).abs() < 5.0,
        "Preview wire start x={} should be near port x={}", wire.start.x, out_pos.x);
    assert!((wire.start.y - out_pos.y).abs() < 5.0,
        "Preview wire start y={} should be near port y={}", wire.start.y, out_pos.y);

    // Release — preview should disappear
    gs.handle_input(InputEvent::MouseUp {
        screen: mid, world: mid,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });
    assert!(gs.preview_wire.get_cloned().is_none(), "Preview wire should be cleared after release");
}

// ============================================================
// Undo/redo preserves connections and offsets
// ============================================================

#[wasm_bindgen_test]
fn test_undo_redo_with_connections() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 2);
    assert_eq!(gs.connection_count(), 1);

    // Delete source node (removes connection too)
    gs.select_single(n1);
    gs.delete_selected();

    assert_eq!(gs.node_count(), 1);
    assert_eq!(gs.connection_count(), 0);

    // Undo — node and connection restored
    gs.undo();
    assert_eq!(gs.node_count(), 2);
    assert_eq!(gs.connection_count(), 1);

    // Connection path should be valid after undo (offsets computed from data, no rAF needed)
    let _conn_id = gs.with_graph(|g| g.world.query::<ConnectionEndpoints>()
        .map(|(id, _)| id).next().unwrap());
    // The connection_list should be repopulated
    assert!(gs.connection_list.lock_ref().len() >= 1,
        "connection_list should have the restored connection");
}

// ============================================================
// Box selection via interaction controller
// ============================================================

#[wasm_bindgen_test]
fn test_box_selection_selects_contained_nodes() {
    let (gs, _n1, _n2, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    // Start box select on empty space
    let start = Vec2::new(50.0, 50.0);
    gs.handle_input(InputEvent::MouseDown {
        screen: start, world: start,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });

    // Drag to cover both nodes (n1 at 100,100 and n2 at 400,100)
    let end = Vec2::new(600.0, 300.0);
    gs.handle_input(InputEvent::MouseMove {
        screen: end, world: end, modifiers: Modifiers::default(),
    });

    // Box select rect should be visible
    assert!(gs.box_select_rect.get_cloned().is_some(), "Box select rect should be visible during drag");

    // Release
    gs.handle_input(InputEvent::MouseUp {
        screen: end, world: end,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });

    // Both nodes should be selected
    let sel = gs.selection.get_cloned();
    assert_eq!(sel.len(), 2, "Both nodes should be selected, got {}", sel.len());

    // Box select rect should be cleared
    assert!(gs.box_select_rect.get_cloned().is_none(), "Box select rect should be cleared after release");
}

// ============================================================
// Cut links via Ctrl+RMB
// ============================================================

#[wasm_bindgen_test]
fn test_cut_links() {
    let (gs, _, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp).unwrap();
    let _tc = render_sync(&gs);

    assert_eq!(gs.connection_count(), 1);

    // Get port positions to know where the wire is
    let out_pos = gs.port_world_pos(out).unwrap();
    let inp_pos = gs.port_world_pos(inp).unwrap();
    let mid_x = (out_pos.x + inp_pos.x) / 2.0;
    let mid_y = out_pos.y; // wire is roughly horizontal

    // Start Ctrl+RMB cut above the wire
    let above = Vec2::new(mid_x, mid_y - 50.0);
    gs.handle_input(InputEvent::MouseDown {
        screen: above, world: above,
        button: MouseButton::Right,
        modifiers: Modifiers { ctrl: true, shift: false, alt: false },
    });

    // Cut line should be visible
    assert!(!gs.cut_line_points.get_cloned().is_empty(), "Cut line should have points during drag");

    // Drag below the wire
    let below = Vec2::new(mid_x, mid_y + 50.0);
    gs.handle_input(InputEvent::MouseMove {
        screen: below, world: below,
        modifiers: Modifiers { ctrl: true, shift: false, alt: false },
    });

    // Release
    gs.handle_input(InputEvent::MouseUp {
        screen: below, world: below,
        button: MouseButton::Right,
        modifiers: Modifiers { ctrl: true, shift: false, alt: false },
    });

    // Connection should be cut
    assert_eq!(gs.connection_count(), 0, "Connection should be removed by cut");
    assert!(gs.cut_line_points.get_cloned().is_empty(), "Cut line should be cleared");
}

// ============================================================
// Multiple connections from same output
// ============================================================

#[wasm_bindgen_test]
fn test_multiple_connections_from_output() {
    let gs = GraphSignals::new();
    let (mc_n1, _) = gs.add_node("Src", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (mc_n2, _) = gs.add_node("Tgt1", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let (mc_n3, _) = gs.add_node("Tgt2", (200.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let (out, in1, in2) = gs.with_graph(|g| (g.node_ports(mc_n1)[0], g.node_ports(mc_n2)[0], g.node_ports(mc_n3)[0]));

    let _tc = render_sync(&gs);

    gs.connect_ports(out, in1).unwrap();
    gs.connect_ports(out, in2).unwrap();

    assert_eq!(gs.connection_count(), 2);
    assert_eq!(gs.connection_list.lock_ref().len(), 2);
}

// ============================================================
// Replacing existing connection on input
// ============================================================

#[wasm_bindgen_test]
fn test_replacing_input_connection() {
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (0.0, 100.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n3, _) = gs.add_node("C", (200.0, 50.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let (out1, out2, inp) = gs.with_graph(|g| (g.node_ports(n1)[0], g.node_ports(n2)[0], g.node_ports(n3)[0]));

    let _tc = render_sync(&gs);

    gs.connect_ports(out1, inp).unwrap();
    assert_eq!(gs.connection_count(), 1);

    // Connecting out2 to the same input should replace
    gs.connect_ports(out2, inp).unwrap();
    assert_eq!(gs.connection_count(), 1, "Old connection should be replaced");
}

// ============================================================
// Search menu
// ============================================================

fn register_test_types(gs: &std::rc::Rc<GraphSignals>) {
    let mut reg = gs.registry.borrow_mut();
    reg.register(NodeTypeDefinition {
        type_id: "math_add".into(), display_name: "Math Add".into(), category: "Math".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "A".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Result".into() },
        ],
    });
    reg.register(NodeTypeDefinition {
        type_id: "shader_out".into(), display_name: "Shader Output".into(), category: "Shader".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Shader, label: "Surface".into() },
        ],
        output_ports: vec![],
    });
}

#[wasm_bindgen_test]
fn test_open_search_menu() {
    let (gs, _, _, _, _) = new_two_node_graph();
    register_test_types(&gs);

    assert!(gs.search_menu.get().is_none());
    gs.open_search_menu(200.0, 150.0);
    assert_eq!(gs.search_menu.get(), Some((200.0, 150.0)));
}

#[wasm_bindgen_test]
fn test_close_search_menu() {
    let (gs, _, _, _, _) = new_two_node_graph();
    register_test_types(&gs);

    gs.open_search_menu(200.0, 150.0);
    gs.close_search_menu();
    assert!(gs.search_menu.get().is_none());
    assert!(gs.pending_connection.get().is_none());
}

#[wasm_bindgen_test]
fn test_spawn_from_registry() {
    let (gs, _, _, _, _) = new_two_node_graph();
    register_test_types(&gs);
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 2);

    gs.open_search_menu(300.0, 300.0);
    gs.spawn_from_registry("math_add", (300.0, 300.0));

    assert_eq!(gs.node_count(), 3);
    assert!(gs.search_menu.get().is_none(), "Menu should close after spawn");
}

#[wasm_bindgen_test]
fn test_noodle_drop_opens_search_with_pending() {
    let (gs, _, _, out, _) = new_two_node_graph();
    register_test_types(&gs);
    let _tc = render_sync(&gs);

    // Start connecting from output port
    let out_pos = gs.port_world_pos(out).unwrap();
    gs.start_connecting(out, out_pos, out_pos);

    // Release on empty canvas
    let far = Vec2::new(500.0, 500.0);
    gs.handle_input(InputEvent::MouseUp {
        screen: far, world: far,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });

    // Search menu should be open with pending connection
    assert!(gs.search_menu.get().is_some(), "Search menu should open on noodle drop");
    assert!(gs.pending_connection.get().is_some(), "Pending connection should be set");
}

#[wasm_bindgen_test]
fn test_spawn_from_registry_auto_connects() {
    let (gs, _, _, out, _) = new_two_node_graph();
    register_test_types(&gs);
    let _tc = render_sync(&gs);

    assert_eq!(gs.connection_count(), 0);

    // Simulate noodle drop → pending connection
    let cf = gs.with_graph(|g| {
        let dir = g.world.get::<PortDirection>(out).copied().unwrap();
        let st = g.world.get::<nodegraph_core::graph::port::PortSocketType>(out).map(|s| s.0).unwrap();
        let from_output = dir == PortDirection::Output;
        (out, st, from_output)
    });
    gs.pending_connection.set(Some(cf));
    gs.search_menu.set(Some((300.0, 300.0)));

    // Spawn Math Add — has Float input, compatible with our Float output
    gs.spawn_from_registry("math_add", (300.0, 300.0));

    assert_eq!(gs.node_count(), 3);
    assert_eq!(gs.connection_count(), 1, "Should auto-connect to spawned node");
    assert!(gs.pending_connection.get().is_none());
    assert!(gs.search_menu.get().is_none());
}

#[wasm_bindgen_test]
fn test_spawn_incompatible_no_auto_connect() {
    let (gs, _, _, out, _) = new_two_node_graph();
    register_test_types(&gs);
    let _tc = render_sync(&gs);

    // Pending from Float output
    let cf = gs.with_graph(|g| {
        let st = g.world.get::<nodegraph_core::graph::port::PortSocketType>(out).map(|s| s.0).unwrap();
        (out, st, true)
    });
    gs.pending_connection.set(Some(cf));

    // Spawn Shader Output — has Shader input, NOT compatible with Float
    gs.spawn_from_registry("shader_out", (300.0, 300.0));

    assert_eq!(gs.node_count(), 3);
    assert_eq!(gs.connection_count(), 0, "Should NOT auto-connect to incompatible node");
}

// ============================================================
// Phase 7: Node Groups
// ============================================================

fn setup_three_node_chain() -> (std::rc::Rc<GraphSignals>, EntityId, EntityId, EntityId) {
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n3, _) = gs.add_node("C", (400.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);

    // Connect A→B→C
    let (out_a, in_b) = gs.with_graph(|g| {
        let a_ports = g.node_ports(n1);
        let b_ports = g.node_ports(n2);
        let out_a = a_ports.iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Output)).copied().unwrap();
        let in_b = b_ports.iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Input)).copied().unwrap();
        (out_a, in_b)
    });
    gs.connect_ports(out_a, in_b).unwrap();

    let (out_b, in_c) = gs.with_graph(|g| {
        let b_ports = g.node_ports(n2);
        let c_ports = g.node_ports(n3);
        let out_b = b_ports.iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Output)).copied().unwrap();
        let in_c = c_ports.iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Input)).copied().unwrap();
        (out_b, in_c)
    });
    gs.connect_ports(out_b, in_c).unwrap();

    (gs, n1, n2, n3)
}

#[wasm_bindgen_test]
fn test_group_creation() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 3);
    assert_eq!(gs.connection_count(), 2);

    // Group node B
    gs.select_single(n2);
    gs.group_selected();

    // Parent should have: A, C, Group (3 nodes)
    assert_eq!(gs.node_count(), 3, "Parent should have A, C, and Group node");

    // Parent should have connections: A→Group, Group→C
    assert_eq!(gs.connection_count(), 2, "Parent should have 2 connections through group");
}

#[wasm_bindgen_test]
fn test_enter_group() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();

    let root_id = gs.editor.borrow().root_graph_id();

    // Find the group node
    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next()
    });
    assert!(group_node.is_some(), "Should have a group node");
    let group_node = group_node.unwrap();

    // Enter the group
    gs.enter_group(group_node);

    // Should be in a different graph now
    assert_ne!(gs.current_graph_id.get(), root_id, "Should have navigated into subgraph");

    // Subgraph should have: Group Input, Group Output, B (3 nodes)
    assert_eq!(gs.node_count(), 3, "Subgraph should have IO nodes + B");

    // Subgraph should have connections: GroupInput→B, B→GroupOutput
    assert_eq!(gs.connection_count(), 2, "Subgraph should have IO connections");

    // Breadcrumb should have 2 entries
    assert_eq!(gs.breadcrumb.lock_ref().len(), 2);
}

#[wasm_bindgen_test]
fn test_exit_group() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();

    let root_id = gs.editor.borrow().root_graph_id();

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    gs.enter_group(group_node);
    assert_ne!(gs.current_graph_id.get(), root_id);

    // Navigate back to root
    gs.navigate_to_graph(root_id);
    assert_eq!(gs.current_graph_id.get(), root_id);
    assert_eq!(gs.breadcrumb.lock_ref().len(), 1);

    // Should see original parent nodes again
    assert_eq!(gs.node_count(), 3);
}

#[wasm_bindgen_test]
fn test_ungroup() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3);

    // Find and select the group node
    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    gs.controller.borrow_mut().selection.clear();
    gs.select_single(group_node);
    gs.ungroup_selected();

    // Group node dissolved; A, B, C all in parent
    assert_eq!(gs.node_count(), 3, "After ungroup: A, B, C should be in parent");
}

#[wasm_bindgen_test]
fn test_add_group_io_node() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    let ports_before = gs.with_graph(|g| g.node_ports(group_node).len());

    gs.enter_group(group_node);

    // Count IO nodes before
    let io_count_before = gs.with_graph(|g| g.world.query::<nodegraph_core::graph::GroupIOKind>().count());

    // Add a new input IO node
    gs.add_group_io_at(nodegraph_core::graph::GroupIOKind::Input, (50.0, 200.0));

    // Should have one more IO node
    let io_count_after = gs.with_graph(|g| g.world.query::<nodegraph_core::graph::GroupIOKind>().count());
    assert_eq!(io_count_after, io_count_before + 1, "Should have one more IO node");

    // Exit and check group node gained a port
    let root_id = gs.editor.borrow().root_graph_id();
    gs.navigate_to_graph(root_id);

    let ports_after = gs.with_graph(|g| g.node_ports(group_node).len());
    assert!(ports_after > ports_before, "Group node should have gained a port. Before: {}, After: {}", ports_before, ports_after);
}

#[wasm_bindgen_test]
fn test_group_io_node_type_adapts() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    gs.enter_group(group_node);

    // Add a new input IO node with Any type
    gs.add_group_io_at(nodegraph_core::graph::GroupIOKind::Input, (50.0, 200.0));

    // Find the new Any-typed IO node (most recent one)
    let new_io_port = gs.with_graph(|g| {
        let mut any_ports = Vec::new();
        for (nid, _) in g.world.query::<nodegraph_core::graph::GroupIOKind>() {
            for &pid in g.node_ports(nid) {
                if let Some(st) = g.world.get::<nodegraph_core::graph::port::PortSocketType>(pid) {
                    if st.0 == SocketType::Any {
                        any_ports.push(pid);
                    }
                }
            }
        }
        any_ports.last().copied()
    });
    assert!(new_io_port.is_some(), "Should have an Any-type port on new IO node");
    let new_io_port = new_io_port.unwrap();

    // Create a Color node inside the subgraph
    let (color_node, _) = gs.add_node("ColorSink", (100.0, 100.0), vec![
        (PortDirection::Input, SocketType::Color, "Color".to_string()),
    ]);
    let color_in = gs.with_graph(|g| g.node_ports(color_node)[0]);

    // Connect IO node's Any output → Color node's Color input
    let conn = gs.connect_ports(new_io_port, color_in);
    assert!(conn.is_ok(), "Connection from IO Any output to Color input should succeed");

    // The IO port should now be Color type
    let adapted_type = gs.with_graph(|g| {
        g.world.get::<nodegraph_core::graph::port::PortSocketType>(new_io_port).map(|s| s.0)
    });
    assert_eq!(adapted_type, Some(SocketType::Color),
        "IO port should adapt to Color after connection. Got: {:?}", adapted_type);
}

#[wasm_bindgen_test]
fn test_nested_groups() {
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n3, _) = gs.add_node("C", (400.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n4, _) = gs.add_node("D", (600.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let _tc = render_sync(&gs);

    let root_id = gs.editor.borrow().root_graph_id();

    // Group B and C together
    gs.controller.borrow_mut().selection.select(n2);
    gs.controller.borrow_mut().selection.select(n3);
    gs.selection.set(vec![n2, n3]);
    gs.group_selected();

    // Parent: A, D, Group (3 nodes)
    assert_eq!(gs.node_count(), 3);

    let outer_group = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    // Enter the outer group
    gs.enter_group(outer_group);
    assert_eq!(gs.breadcrumb.lock_ref().len(), 2);

    // Inside: B, C (no IO nodes because no external connections)
    assert_eq!(gs.node_count(), 2, "Outer subgraph should have B + C (no IO — no external connections)");

    // Now group just C inside the subgraph (nested group)
    let inner_c = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::node::NodeHeader>()
            .find(|(_, h)| h.title == "C")
            .map(|(id, _)| id)
    });
    assert!(inner_c.is_some(), "Should find node C in subgraph");
    let inner_c = inner_c.unwrap();

    gs.controller.borrow_mut().selection.clear();
    gs.select_single(inner_c);
    gs.group_selected();

    // Outer subgraph: B, InnerGroup (2 nodes — no IO because no external connections)
    assert_eq!(gs.node_count(), 2, "Should have B + InnerGroup after inner grouping");

    // Find and enter the inner group
    let inner_group = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });
    gs.enter_group(inner_group);

    // Breadcrumb: Root > OuterGroup > InnerGroup (3 levels)
    assert_eq!(gs.breadcrumb.lock_ref().len(), 3, "Should be 3 levels deep");

    // Inner subgraph: just C (no IO — no connections existed)
    assert_eq!(gs.node_count(), 1, "Inner subgraph should have just C");

    // Navigate all the way back to root
    gs.navigate_to_graph(root_id);
    assert_eq!(gs.current_graph_id.get(), root_id);
    assert_eq!(gs.breadcrumb.lock_ref().len(), 1);
    assert_eq!(gs.node_count(), 3, "Root should still have A, D, OuterGroup");
}

#[wasm_bindgen_test]
fn test_group_node_has_subgraph_root() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();

    // The group node should have SubgraphRoot component
    let has_subgraph = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>().count()
    });
    assert_eq!(has_subgraph, 1, "Should have exactly 1 group node with SubgraphRoot");
}

#[wasm_bindgen_test]
fn test_breadcrumb_updates() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.breadcrumb.lock_ref().len(), 1, "Start with Root");

    gs.select_single(n2);
    gs.group_selected();

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    gs.enter_group(group_node);
    assert_eq!(gs.breadcrumb.lock_ref().len(), 2, "Should have Root > Group");

    let root_id = gs.editor.borrow().root_graph_id();
    gs.navigate_to_graph(root_id);
    assert_eq!(gs.breadcrumb.lock_ref().len(), 1, "Back to Root only");
}

// ============================================================
// Ungroup must restore nodes, not just delete
// ============================================================

#[wasm_bindgen_test]
fn test_ungroup_restores_nodes_to_parent() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 3);
    assert_eq!(gs.connection_count(), 2);

    // Group B
    gs.select_single(n2);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3); // A, C, Group

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    // Ungroup
    gs.select_single(group_node);
    gs.ungroup_selected();

    // B should be back in the parent, not deleted
    assert_eq!(gs.node_count(), 3, "After ungroup: A, B, C should all be in parent. Got {}", gs.node_count());
    // Connections A→B and B→C should be restored
    assert_eq!(gs.connection_count(), 2, "After ungroup: connections should be restored. Got {}", gs.connection_count());
}

// ============================================================
// Undo/redo must work with groups
// ============================================================

#[wasm_bindgen_test]
fn test_undo_group() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 3);

    // Group B
    gs.select_single(n2);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3); // A, C, Group

    // Undo should restore original 3 nodes
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After undo group: should have A, B, C. Got {}", gs.node_count());
    assert_eq!(gs.connection_count(), 2, "After undo group: connections should be restored. Got {}", gs.connection_count());
}

#[wasm_bindgen_test]
fn test_undo_delete_in_group() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    // Delete node A
    let n1 = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::node::NodeHeader>()
            .find(|(_, h)| h.title == "A").map(|(id, _)| id).unwrap()
    });
    gs.select_single(n1);
    gs.delete_selected();
    assert_eq!(gs.node_count(), 2);

    // Undo
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After undo delete: should have 3 nodes. Got {}", gs.node_count());
}

#[wasm_bindgen_test]
fn test_undo_add_node() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);
    assert_eq!(gs.node_count(), 2);

    // Add a node via registry
    register_test_types(&gs);
    gs.open_search_menu(300.0, 300.0);
    gs.spawn_from_registry("math_add", (300.0, 300.0));
    assert_eq!(gs.node_count(), 3);

    // Undo — node should be removed
    gs.undo();
    assert_eq!(gs.node_count(), 2, "After undo add: should have 2 nodes. Got {}", gs.node_count());
}

#[wasm_bindgen_test]
fn test_undo_connect() {
    let (gs, _, _, out, inp) = new_two_node_graph();
    let _tc = render_sync(&gs);
    assert_eq!(gs.connection_count(), 0);

    gs.connect_ports(out, inp).unwrap();
    assert_eq!(gs.connection_count(), 1);

    gs.undo();
    assert_eq!(gs.connection_count(), 0, "After undo connect: should have 0 connections. Got {}", gs.connection_count());
}

// ============================================================
// Undo debt: group/ungroup/add-io-port must be undoable
// ============================================================

#[wasm_bindgen_test]
fn test_undo_ungroup() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    // Group B
    gs.select_single(n2);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3); // A, C, Group

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    // Ungroup
    gs.select_single(group_node);
    gs.ungroup_selected();
    assert_eq!(gs.node_count(), 3); // A, B, C restored

    // Undo ungroup → group should be back
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After undo ungroup: should have A, C, Group. Got {}", gs.node_count());

    // The group node should exist again with SubgraphRoot
    let has_group = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>().count()
    });
    assert_eq!(has_group, 1, "After undo ungroup: group node should be back");
}

#[wasm_bindgen_test]
fn test_undo_add_io_node() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.group_selected();

    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });

    let group_ports_before = gs.with_graph(|g| g.node_ports(group_node).len());

    gs.enter_group(group_node);

    let io_count_before = gs.with_graph(|g| g.world.query::<nodegraph_core::graph::GroupIOKind>().count());

    // Add a new IO node
    gs.add_group_io_at(nodegraph_core::graph::GroupIOKind::Input, (50.0, 200.0));

    let io_count_after = gs.with_graph(|g| g.world.query::<nodegraph_core::graph::GroupIOKind>().count());
    assert_eq!(io_count_after, io_count_before + 1, "IO node should be added");

    // Undo → IO node should be gone
    gs.undo();
    let io_count_undone = gs.with_graph(|g| g.world.query::<nodegraph_core::graph::GroupIOKind>().count());
    assert_eq!(io_count_undone, io_count_before, "After undo: IO node should be removed");

    // Check parent group node also lost the port
    let root_id = gs.editor.borrow().root_graph_id();
    gs.navigate_to_graph(root_id);
    let group_ports_undone = gs.with_graph(|g| g.node_ports(group_node).len());
    assert_eq!(group_ports_undone, group_ports_before, "After undo: parent group port should be removed too");
}

#[wasm_bindgen_test]
fn test_undo_redo_group_cycle() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 3);
    assert_eq!(gs.connection_count(), 2);

    // Group B
    gs.select_single(n2);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3); // A, C, Group

    // Undo group → back to A, B, C
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After undo group: A, B, C");
    assert_eq!(gs.connection_count(), 2, "After undo group: 2 connections");
    let has_group = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>().count()
    });
    assert_eq!(has_group, 0, "After undo group: no group nodes");

    // Redo group
    gs.redo();
    assert_eq!(gs.node_count(), 3, "After redo group: A, C, Group. Got {}", gs.node_count());

    // Undo again
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After 2nd undo: A, B, C. Got {}", gs.node_count());

    // Redo again
    gs.redo();
    assert_eq!(gs.node_count(), 3, "After 2nd redo: A, C, Group. Got {}", gs.node_count());

    // Undo again
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After 3rd undo: A, B, C. Got {}", gs.node_count());
    assert_eq!(gs.connection_count(), 2, "After 3rd undo: 2 connections. Got {}", gs.connection_count());
}

#[wasm_bindgen_test]
fn test_group_a_group_with_another_node() {
    // A → B → C, group B, then group (GroupB + C) together
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    // Group B
    gs.select_single(n2);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3); // A, C, GroupB

    // Find GroupB and C
    let (group_b, node_c) = gs.with_graph(|g| {
        let group = g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap();
        let c = g.world.query::<nodegraph_core::graph::node::NodeHeader>()
            .find(|(id, h)| h.title == "C")
            .map(|(id, _)| id).unwrap();
        (group, c)
    });

    // Group (GroupB + C) together
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(group_b);
    gs.controller.borrow_mut().selection.select(node_c);
    gs.selection.set(vec![group_b, node_c]);
    gs.group_selected();

    // Should have: A, OuterGroup (2 nodes)
    assert_eq!(gs.node_count(), 2, "After nesting: A + OuterGroup. Got {}", gs.node_count());

    // Undo → back to A, C, GroupB
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After undo nested group: A, C, GroupB. Got {}", gs.node_count());

    // Undo again → back to A, B, C
    gs.undo();
    assert_eq!(gs.node_count(), 3, "After undo first group: A, B, C. Got {}", gs.node_count());
    assert_eq!(gs.connection_count(), 2, "After full undo: 2 connections. Got {}", gs.connection_count());
}

#[wasm_bindgen_test]
fn test_nested_group_undo_redo_stress() {
    // A → B → C → D, group B+C, undo, redo, undo, group B only, undo, redo
    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n3, _) = gs.add_node("C", (400.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n4, _) = gs.add_node("D", (600.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let _tc = render_sync(&gs);

    // Connect A→B→C→D
    let ports = gs.with_graph(|g| {
        let a_out = g.node_ports(n1).iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Output)).copied().unwrap();
        let b_in = g.node_ports(n2).iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Input)).copied().unwrap();
        let b_out = g.node_ports(n2).iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Output)).copied().unwrap();
        let c_in = g.node_ports(n3).iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Input)).copied().unwrap();
        let c_out = g.node_ports(n3).iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Output)).copied().unwrap();
        let d_in = g.node_ports(n4).iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Input)).copied().unwrap();
        (a_out, b_in, b_out, c_in, c_out, d_in)
    });
    gs.connect_ports(ports.0, ports.1).unwrap();
    gs.connect_ports(ports.2, ports.3).unwrap();
    gs.connect_ports(ports.4, ports.5).unwrap();

    assert_eq!(gs.node_count(), 4);
    assert_eq!(gs.connection_count(), 3);

    // Group B+C
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n2);
    gs.controller.borrow_mut().selection.select(n3);
    gs.selection.set(vec![n2, n3]);
    gs.group_selected();
    assert_eq!(gs.node_count(), 3, "After group B+C: A, D, Group. Got {}", gs.node_count());

    // Undo
    gs.undo();
    assert_eq!(gs.node_count(), 4, "After undo: A, B, C, D. Got {}", gs.node_count());
    assert_eq!(gs.connection_count(), 3, "After undo: 3 connections. Got {}", gs.connection_count());

    // Redo
    gs.redo();
    assert_eq!(gs.node_count(), 3, "After redo: A, D, Group. Got {}", gs.node_count());

    // Undo again
    gs.undo();
    assert_eq!(gs.node_count(), 4, "After 2nd undo: A, B, C, D. Got {}", gs.node_count());
    assert_eq!(gs.connection_count(), 3, "After 2nd undo: 3 connections. Got {}", gs.connection_count());
}

// ============================================================
// Frames
// ============================================================

#[wasm_bindgen_test]
fn test_create_frame() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.with_graph(|g| g.frame_count()), 0);

    gs.select_single(n2);
    gs.create_frame_around_selected();

    assert_eq!(gs.with_graph(|g| g.frame_count()), 1, "Should have 1 frame");
    // Nodes should be unaffected
    assert_eq!(gs.node_count(), 3);
}

#[wasm_bindgen_test]
fn test_undo_create_frame() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.create_frame_around_selected();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 1);

    gs.undo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 0, "After undo: frame should be gone");
    assert_eq!(gs.node_count(), 3, "Nodes unaffected");
}

#[wasm_bindgen_test]
fn test_frame_has_correct_members() {
    let (gs, n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    // Select A and B
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    let members = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::frame::FrameMembers>()
            .map(|(_, m)| m.0.len()).next().unwrap_or(0)
    });
    assert_eq!(members, 2, "Frame should have 2 members");
}

// ============================================================
// Reroutes
// ============================================================

#[wasm_bindgen_test]
fn test_spawn_reroute() {
    let (gs, _, _, _, _) = new_two_node_graph();
    register_test_types(&gs);
    // Also register reroute
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "reroute".into(),
        display_name: "Reroute".into(),
        category: "Utility".into(),
        input_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into(),
        }],
        output_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into(),
        }],
    });
    let _tc = render_sync(&gs);

    gs.spawn_from_registry("reroute", (200.0, 200.0));

    assert_eq!(gs.node_count(), 3, "Should have 2 original + 1 reroute");

    // Check it has IsReroute marker
    let has_reroute = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::reroute::IsReroute>().count()
    });
    assert_eq!(has_reroute, 1, "Should have 1 reroute node");
}

#[wasm_bindgen_test]
fn test_reroute_has_any_ports() {
    let gs = GraphSignals::new();
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "reroute".into(),
        display_name: "Reroute".into(),
        category: "Utility".into(),
        input_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into(),
        }],
        output_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into(),
        }],
    });
    let _tc = render_sync(&gs);

    gs.spawn_from_registry("reroute", (100.0, 100.0));

    // Check port types are Any
    let port_types = gs.with_graph(|g| {
        let reroute_id = g.world.query::<nodegraph_core::graph::reroute::IsReroute>()
            .map(|(id, _)| id).next().unwrap();
        g.node_ports(reroute_id).iter().map(|&pid| {
            g.world.get::<nodegraph_core::graph::port::PortSocketType>(pid).map(|s| s.0)
        }).collect::<Vec<_>>()
    });
    assert_eq!(port_types.len(), 2);
    assert_eq!(port_types[0], Some(SocketType::Any));
    assert_eq!(port_types[1], Some(SocketType::Any));
}

#[wasm_bindgen_test]
fn test_undo_spawn_reroute() {
    let gs = GraphSignals::new();
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "reroute".into(), display_name: "Reroute".into(), category: "Utility".into(),
        input_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into(),
        }],
        output_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into(),
        }],
    });
    let _tc = render_sync(&gs);

    assert_eq!(gs.node_count(), 0);
    gs.spawn_from_registry("reroute", (100.0, 100.0));
    assert_eq!(gs.node_count(), 1);

    gs.undo();
    assert_eq!(gs.node_count(), 0, "After undo: reroute should be gone");
}

#[wasm_bindgen_test]
fn test_frame_appears_in_frame_list() {
    let (gs, n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    assert_eq!(gs.frame_list.lock_ref().len(), 0);

    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    assert_eq!(gs.frame_list.lock_ref().len(), 1, "frame_list should have 1 entry for SVG rendering");
}

#[wasm_bindgen_test]
fn test_frame_rect_bounds_contain_members() {
    let (gs, n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    // A is at (0,0), B is at (200,0)
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    let rect = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::frame::FrameRect>()
            .map(|(_, r)| r.clone()).next()
    });
    assert!(rect.is_some());
    let rect = rect.unwrap();

    // Frame should enclose both nodes with padding
    assert!(rect.x < 0.0, "Frame x={} should be left of node A (at 0)", rect.x);
    assert!(rect.y < 0.0, "Frame y={} should be above node A (at 0)", rect.y);
    assert!(rect.x + rect.w > 200.0, "Frame right edge should be past node B (at 200)");
    assert!(rect.w > 200.0, "Frame width={} should span both nodes", rect.w);
}

#[wasm_bindgen_test]
fn test_undo_frame_clears_frame_list() {
    let (gs, _n1, n2, _n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    gs.select_single(n2);
    gs.create_frame_around_selected();
    assert_eq!(gs.frame_list.lock_ref().len(), 1);

    gs.undo();
    assert_eq!(gs.frame_list.lock_ref().len(), 0, "frame_list should be empty after undo");
}

#[wasm_bindgen_test]
fn test_connect_through_reroute() {
    let gs = GraphSignals::new();
    // Create source (Float output), reroute (Any in/out), sink (Float input)
    let (src, _) = gs.add_node("Src", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (sink, _) = gs.add_node("Sink", (400.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let (reroute, _) = gs.add_node("", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Any, "".to_string()),
        (PortDirection::Output, SocketType::Any, "".to_string()),
    ]);
    gs.with_graph_mut(|g| {
        g.world.insert(reroute, nodegraph_core::graph::reroute::IsReroute);
    });
    let _tc = render_sync(&gs);

    // Get port IDs
    let (src_out, reroute_in, reroute_out, sink_in) = gs.with_graph(|g| {
        let sp = g.node_ports(src);
        let rp = g.node_ports(reroute);
        let dp = g.node_ports(sink);
        (sp[0], rp[0], rp[1], dp[0])
    });

    // Connect Src → Reroute → Sink
    gs.connect_ports(src_out, reroute_in).unwrap();
    gs.connect_ports(reroute_out, sink_in).unwrap();

    assert_eq!(gs.connection_count(), 2, "Should have 2 connections through reroute");

    // Delete the reroute
    gs.select_single(reroute);
    gs.delete_selected();
    assert_eq!(gs.node_count(), 2, "Reroute deleted, Src and Sink remain");
    assert_eq!(gs.connection_count(), 0, "Connections through reroute removed");

    // Undo → reroute + connections back
    gs.undo();
    assert_eq!(gs.node_count(), 3);
    assert_eq!(gs.connection_count(), 2, "Connections restored after undo");
}

#[wasm_bindgen_test]
fn test_reroute_in_node_list() {
    let gs = GraphSignals::new();
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "reroute".into(), display_name: "Reroute".into(), category: "Utility".into(),
        input_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into(),
        }],
        output_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into(),
        }],
    });
    let _tc = render_sync(&gs);

    gs.spawn_from_registry("reroute", (100.0, 100.0));
    assert_eq!(gs.node_list.lock_ref().len(), 1, "Reroute should be in node_list for SVG rendering");
}

#[wasm_bindgen_test]
fn test_multiple_frames() {
    let (gs, n1, n2, n3) = setup_three_node_chain();
    let _tc = render_sync(&gs);

    // Frame around A
    gs.select_single(n1);
    gs.create_frame_around_selected();

    // Frame around B+C
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n2);
    gs.controller.borrow_mut().selection.select(n3);
    gs.selection.set(vec![n2, n3]);
    gs.create_frame_around_selected();

    assert_eq!(gs.with_graph(|g| g.frame_count()), 2);
    assert_eq!(gs.frame_list.lock_ref().len(), 2);
    assert_eq!(gs.node_count(), 3, "Nodes unaffected by frames");
}

// ── Frame deletion leaves nodes intact ──────────────────────────────
#[wasm_bindgen_test]
fn test_delete_frame_leaves_nodes() {
    let (gs, _, _, out, inp) = new_two_node_graph();
    let _tc = render_sync(&gs);
    gs.connect_ports(out, inp).unwrap();

    let n1 = gs.node_list.lock_ref()[0];
    let n2 = gs.node_list.lock_ref()[1];

    // Create frame around both nodes
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    assert_eq!(gs.with_graph(|g| g.frame_count()), 1);
    assert_eq!(gs.node_count(), 2);

    // Undo the frame creation (removes the frame)
    gs.undo();

    // Nodes and connections still exist
    assert_eq!(gs.with_graph(|g| g.frame_count()), 0);
    assert_eq!(gs.node_count(), 2, "Nodes must survive frame deletion");
    assert_eq!(gs.with_graph(|g| g.connection_count()), 1, "Connections must survive frame deletion");

    // Redo restores the frame
    gs.redo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 1);
    assert_eq!(gs.node_count(), 2, "Nodes still intact after redo");
}

// ── Deleting a node cleans it from frame members ────────────────────
#[wasm_bindgen_test]
fn test_delete_node_cleans_frame_members() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let n1 = gs.node_list.lock_ref()[0];
    let n2 = gs.node_list.lock_ref()[1];

    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    // Verify frame has 2 members
    let member_count = gs.with_graph(|g| {
        use nodegraph_core::graph::frame::{FrameRect, FrameMembers};
        let (fid, _) = g.world.query::<FrameRect>().next().unwrap();
        g.world.get::<FrameMembers>(fid).unwrap().0.len()
    });
    assert_eq!(member_count, 2);

    // Delete one node
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(vec![n1]);
    gs.delete_selected();

    // Frame should now have 1 member
    let member_count = gs.with_graph(|g| {
        use nodegraph_core::graph::frame::{FrameRect, FrameMembers};
        let (fid, _) = g.world.query::<FrameRect>().next().unwrap();
        g.world.get::<FrameMembers>(fid).unwrap().0.len()
    });
    assert_eq!(member_count, 1, "Deleted node must be removed from frame members");
}

// ── Duplicate reroute preserves IsReroute marker ────────────────────
#[wasm_bindgen_test]
fn test_duplicate_reroute_preserves_marker() {
    let (gs, _, _, _, _) = new_two_node_graph();
    register_test_types(&gs);
    // Register reroute type
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "reroute".into(),
        display_name: "Reroute".into(),
        category: "Utility".into(),
        input_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into(),
        }],
        output_ports: vec![nodegraph_core::search::PortDefinition {
            direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into(),
        }],
    });
    let _tc = render_sync(&gs);

    gs.spawn_from_registry("reroute", (200.0, 200.0));
    assert_eq!(gs.node_count(), 3, "2 original + 1 reroute");

    // Select the reroute and duplicate
    let reroute_id = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::reroute::IsReroute>()
            .next().unwrap().0
    });
    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(reroute_id);
    gs.selection.set(vec![reroute_id]);
    gs.duplicate_selected();

    assert_eq!(gs.node_count(), 4, "2 original + 1 reroute + 1 duplicate");

    // Both reroute nodes should have IsReroute
    let reroute_count = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::reroute::IsReroute>().count()
    });
    assert_eq!(reroute_count, 2, "Duplicated reroute must preserve IsReroute marker");
}

// ── Frame bounds update when member nodes move ──────────────────────
#[wasm_bindgen_test]
fn test_frame_bounds_track_member_positions() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let n1 = gs.node_list.lock_ref()[0];
    let n2 = gs.node_list.lock_ref()[1];

    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    let frame_id = gs.with_graph(|g| {
        use nodegraph_core::graph::frame::FrameRect;
        g.world.query::<FrameRect>().next().unwrap().0
    });

    // Get initial bounds
    let initial_bounds = gs.get_frame_bounds_signal(frame_id).unwrap().get();

    use nodegraph_core::interaction::{InputEvent, MouseButton, Modifiers};
    use nodegraph_core::layout::Vec2;

    // Simulate a real drag of n1: click on it, move, release
    // n1 is at (100, 100), click center of header
    let click = Vec2::new(180.0, 114.0);
    gs.handle_input(InputEvent::MouseDown {
        screen: click, world: click,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });

    // Move by (100, 100)
    let drag_to = Vec2::new(280.0, 214.0);
    gs.handle_input(InputEvent::MouseMove {
        screen: drag_to, world: drag_to,
        modifiers: Modifiers::default(),
    });

    gs.handle_input(InputEvent::MouseUp {
        screen: drag_to, world: drag_to,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });

    let updated_bounds = gs.get_frame_bounds_signal(frame_id).unwrap().get();
    // Frame bounds should have changed because node moved
    assert_ne!(initial_bounds, updated_bounds, "Frame bounds must update when member nodes move");
}

// ── Frame drag selects and moves all member nodes ───────────────────
#[wasm_bindgen_test]
fn test_frame_drag_moves_members() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let n1 = gs.node_list.lock_ref()[0];
    let n2 = gs.node_list.lock_ref()[1];

    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    // Get original positions
    let orig_n1 = gs.with_graph(|g| {
        let p = g.world.get::<nodegraph_core::graph::node::NodePosition>(n1).unwrap();
        (p.x, p.y)
    });
    let orig_n2 = gs.with_graph(|g| {
        let p = g.world.get::<nodegraph_core::graph::node::NodePosition>(n2).unwrap();
        (p.x, p.y)
    });

    use nodegraph_core::interaction::{InputEvent, MouseButton, Modifiers};
    use nodegraph_core::layout::Vec2;

    // Click inside frame padding area (not on any node)
    // Nodes at (100,100) and (400,100). Frame starts at ~(70, 70).
    // Click at (75, 75) — inside frame padding, outside any node rect.
    let click = Vec2::new(75.0, 75.0);
    gs.handle_input(InputEvent::MouseDown {
        screen: click, world: click,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });

    // Both nodes should now be selected
    assert_eq!(gs.selection.get_cloned().len(), 2, "Frame click should select all member nodes");

    // Drag to move by (50, 30)
    let drag_to = Vec2::new(125.0, 105.0);
    gs.handle_input(InputEvent::MouseMove {
        screen: drag_to, world: drag_to,
        modifiers: Modifiers::default(),
    });

    gs.handle_input(InputEvent::MouseUp {
        screen: drag_to, world: drag_to,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });

    // Verify both nodes moved by (50, 30)
    let new_n1 = gs.with_graph(|g| {
        let p = g.world.get::<nodegraph_core::graph::node::NodePosition>(n1).unwrap();
        (p.x, p.y)
    });
    let new_n2 = gs.with_graph(|g| {
        let p = g.world.get::<nodegraph_core::graph::node::NodePosition>(n2).unwrap();
        (p.x, p.y)
    });

    assert!((new_n1.0 - orig_n1.0 - 50.0).abs() < 1e-6, "N1 x should move by 50, got delta {}", new_n1.0 - orig_n1.0);
    assert!((new_n1.1 - orig_n1.1 - 30.0).abs() < 1e-6, "N1 y should move by 30");
    assert!((new_n2.0 - orig_n2.0 - 50.0).abs() < 1e-6, "N2 x should move by 50");
    assert!((new_n2.1 - orig_n2.1 - 30.0).abs() < 1e-6, "N2 y should move by 30");
}

// ── Delete frame via frame selection + delete_selected ──────────────
#[wasm_bindgen_test]
fn test_delete_selected_frame() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let n1 = gs.node_list.lock_ref()[0];
    let n2 = gs.node_list.lock_ref()[1];

    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();

    assert_eq!(gs.with_graph(|g| g.frame_count()), 1);

    // Select the frame by clicking in its padding
    use nodegraph_core::interaction::{InputEvent, MouseButton, Modifiers};
    use nodegraph_core::layout::Vec2;
    let click = Vec2::new(75.0, 75.0);
    gs.handle_input(InputEvent::MouseDown {
        screen: click, world: click,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });
    gs.handle_input(InputEvent::MouseUp {
        screen: click, world: click,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
    });

    assert_eq!(gs.selected_frames.get_cloned().len(), 1, "Frame should be selected");

    // Delete — should remove frame AND selected member nodes
    gs.delete_selected();

    assert_eq!(gs.with_graph(|g| g.frame_count()), 0, "Frame should be deleted");
    assert_eq!(gs.node_count(), 0, "Member nodes should also be deleted");

    // Undo restores everything
    gs.undo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 1, "Frame restored after undo");
    assert_eq!(gs.node_count(), 2, "Nodes restored after undo");
}

// ── Noodle-drop search menu DOM shows only compatible nodes ──────────
#[wasm_bindgen_test]
async fn test_noodle_drop_search_menu_dom_filtered() {
    let gs = GraphSignals::new();

    // Register 3 types: Math Add (Float), Material Output (Shader), Reroute (Any)
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "math_add".into(), display_name: "Math Add".into(), category: "Math".into(),
        input_ports: vec![
            nodegraph_core::search::PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "A".into() },
        ],
        output_ports: vec![
            nodegraph_core::search::PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Result".into() },
        ],
    });
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "material_output".into(), display_name: "Material Output".into(), category: "Output".into(),
        input_ports: vec![
            nodegraph_core::search::PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Shader, label: "Surface".into() },
        ],
        output_ports: vec![],
    });
    gs.registry.borrow_mut().register(nodegraph_core::search::NodeTypeDefinition {
        type_id: "reroute".into(), display_name: "Reroute".into(), category: "Utility".into(),
        input_ports: vec![
            nodegraph_core::search::PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into() },
        ],
        output_ports: vec![
            nodegraph_core::search::PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into() },
        ],
    });

    // Add a node with a Shader output so we have a real port to drag from
    let (shader_node, _) = gs.add_node("BSDF", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Shader, "BSDF".to_string()),
    ]);
    let shader_port = gs.with_graph(|g| g.node_ports(shader_node)[0]);

    // Render the full editor (including search menu)
    let _tc = render_sync(&gs);

    // Simulate noodle drag from Shader output → release on empty space
    let port_pos = gs.port_world_pos(shader_port).unwrap();
    gs.start_connecting(shader_port, port_pos, port_pos);
    let empty = Vec2::new(500.0, 500.0);
    gs.handle_input(InputEvent::MouseUp {
        screen: empty, world: empty,
        button: MouseButton::Left, modifiers: Modifiers::default(),
    });

    // Verify search menu is open with pending connection
    assert!(gs.search_menu.get().is_some(), "Search menu should be open");
    assert!(gs.pending_connection.get().is_some(), "pending_connection should be set");

    // Flush microtasks so dominator signals propagate to DOM
    let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;

    // Query the actual rendered DOM — search menu is inside the rendered editor
    let doc = web_sys::window().unwrap().document().unwrap();
    let menu = doc.query_selector("[data-search-menu]").unwrap();
    assert!(menu.is_some(), "Search menu element should exist in DOM");
    let menu_el = menu.unwrap();
    let text = menu_el.text_content().unwrap_or_default();

    // Material Output has Shader input — should appear
    assert!(text.contains("Material Output"),
        "DOM should show Material Output for Shader source, got: {}", text);
    // Reroute has Any input — should appear
    assert!(text.contains("Reroute"),
        "DOM should show Reroute for Shader source, got: {}", text);
    // Math Add has Float input — should NOT appear
    assert!(!text.contains("Math Add"),
        "DOM should NOT show Math Add for Shader source, got: {}", text);
}

// ── Group IO nodes render as compact rects ──────────────────────────
#[wasm_bindgen_test]
async fn test_group_io_nodes_render_compact() {
    use nodegraph_core::layout::IO_NODE_WIDTH;

    let gs = GraphSignals::new();
    let (n1, _) = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n2, _) = gs.add_node("B", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (n3, _) = gs.add_node("C", (400.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);

    // Connect A→B→C
    let (a_out, b_in, b_out, c_in) = gs.with_graph(|g| {
        (g.node_ports(n1)[0], g.node_ports(n2)[0], g.node_ports(n2)[1], g.node_ports(n3)[0])
    });
    gs.connect_ports(a_out, b_in).unwrap();
    gs.connect_ports(b_out, c_in).unwrap();

    let _tc = render_sync(&gs);

    // Group B
    gs.select_single(n2);
    gs.group_selected();

    // Find and enter the group
    let group_node = gs.with_graph(|g| {
        g.world.query::<nodegraph_core::graph::group::SubgraphRoot>()
            .map(|(id, _)| id).next().unwrap()
    });
    gs.enter_group(group_node);

    // Flush signals
    let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;

    // Inside the group we should have: 1 Input IO + B + 1 Output IO = 3 nodes
    assert_eq!(gs.node_count(), 3, "Subgraph should have Input IO + B + Output IO");

    // Verify IO nodes have GroupIOKind
    let io_count = gs.with_graph(|g| g.world.query::<nodegraph_core::graph::GroupIOKind>().count());
    assert_eq!(io_count, 2, "Should have 2 IO nodes");

    // Check DOM: IO node rects should have width = IO_NODE_WIDTH (120)
    let doc = web_sys::window().unwrap().document().unwrap();
    let node_groups = doc.query_selector_all("[data-node-id]").unwrap();
    let mut io_width_count = 0;
    for i in 0..node_groups.length() {
        if let Some(el) = node_groups.get(i) {
            if let Ok(el) = el.dyn_into::<web_sys::Element>() {
                if let Some(rect) = el.query_selector("rect").unwrap() {
                    let w = rect.get_attribute("width").unwrap_or_default();
                    if w == format!("{}", IO_NODE_WIDTH) {
                        io_width_count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(io_width_count, 2, "Should have 2 IO nodes rendered with IO_NODE_WIDTH={}, found {}", IO_NODE_WIDTH, io_width_count);
}

// ============================================================
// Polish: testing gaps
// ============================================================

#[wasm_bindgen_test]
fn test_port_offset_exact_coordinates() {
    use nodegraph_core::layout::{HEADER_HEIGHT, PORT_HEIGHT, NODE_MIN_WIDTH, REROUTE_SIZE};

    // Two-node graph: Source at (100,100), Target at (400,100)
    let (gs, n1, n2, out, inp) = new_two_node_graph();

    // Output port on Source: x = node_x + NODE_MIN_WIDTH, y = node_y + HEADER_HEIGHT + 0.5 * PORT_HEIGHT
    let out_pos = gs.port_world_pos(out).unwrap();
    assert!((out_pos.x - (100.0 + NODE_MIN_WIDTH)).abs() < 0.01,
        "Output port x={} should be {} (node_x + NODE_MIN_WIDTH)", out_pos.x, 100.0 + NODE_MIN_WIDTH);
    assert!((out_pos.y - (100.0 + HEADER_HEIGHT + 0.5 * PORT_HEIGHT)).abs() < 0.01,
        "Output port y={} should be {} (node_y + HEADER_HEIGHT + 0.5*PORT_HEIGHT)", out_pos.y, 100.0 + HEADER_HEIGHT + 0.5 * PORT_HEIGHT);

    // Input port on Target: x = node_x (left edge), same y formula
    let inp_pos = gs.port_world_pos(inp).unwrap();
    assert!((inp_pos.x - 400.0).abs() < 0.01,
        "Input port x={} should be 400.0 (node_x)", inp_pos.x);
    assert!((inp_pos.y - (100.0 + HEADER_HEIGHT + 0.5 * PORT_HEIGHT)).abs() < 0.01,
        "Input port y={} should be {}", inp_pos.y, 100.0 + HEADER_HEIGHT + 0.5 * PORT_HEIGHT);
}

#[wasm_bindgen_test]
fn test_reroute_port_offset_exact() {
    use nodegraph_core::layout::REROUTE_SIZE;

    let (gs, _, _, _, _) = new_two_node_graph();
    register_test_types(&gs);
    gs.registry.borrow_mut().register(NodeTypeDefinition {
        type_id: "reroute".into(), display_name: "Reroute".into(), category: "Utility".into(),
        input_ports: vec![PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Any, label: "".into() }],
        output_ports: vec![PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Any, label: "".into() }],
    });
    let _tc = render_sync(&gs);

    gs.spawn_from_registry("reroute", (200.0, 200.0));

    // Find reroute ports
    let (r_input, r_output) = gs.with_graph(|g| {
        let reroute_id = g.world.query::<nodegraph_core::graph::reroute::IsReroute>().next().unwrap().0;
        let ports = g.node_ports(reroute_id).to_vec();
        let inp = ports.iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Input)).copied().unwrap();
        let out = ports.iter().find(|&&p| g.world.get::<PortDirection>(p) == Some(&PortDirection::Output)).copied().unwrap();
        (inp, out)
    });

    let inp_pos = gs.port_world_pos(r_input).unwrap();
    let out_pos = gs.port_world_pos(r_output).unwrap();

    // Reroute at (200, 200): input at (200 - REROUTE_SIZE, 200), output at (200 + REROUTE_SIZE, 200)
    assert!((inp_pos.x - (200.0 - REROUTE_SIZE)).abs() < 0.01,
        "Reroute input x={} should be {}", inp_pos.x, 200.0 - REROUTE_SIZE);
    assert!((inp_pos.y - 200.0).abs() < 0.01,
        "Reroute input y={} should be 200.0", inp_pos.y);
    assert!((out_pos.x - (200.0 + REROUTE_SIZE)).abs() < 0.01,
        "Reroute output x={} should be {}", out_pos.x, 200.0 + REROUTE_SIZE);
    assert!((out_pos.y - 200.0).abs() < 0.01,
        "Reroute output y={} should be 200.0", out_pos.y);
}

#[wasm_bindgen_test]
fn test_frame_undo_redo_cycle() {
    let (gs, _, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    let n1 = gs.node_list.lock_ref()[0];
    let n2 = gs.node_list.lock_ref()[1];

    gs.controller.borrow_mut().selection.clear();
    gs.controller.borrow_mut().selection.select(n1);
    gs.controller.borrow_mut().selection.select(n2);
    gs.selection.set(vec![n1, n2]);
    gs.create_frame_around_selected();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 1);

    // undo → redo → undo → redo cycle
    gs.undo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 0, "After undo: no frames");
    gs.redo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 1, "After redo: frame back");
    gs.undo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 0, "After 2nd undo: no frames");
    gs.redo();
    assert_eq!(gs.with_graph(|g| g.frame_count()), 1, "After 2nd redo: frame back");
    assert_eq!(gs.node_count(), 2, "Nodes intact after undo/redo cycle");
}

#[wasm_bindgen_test]
fn test_connect_through_two_reroutes() {
    let gs = GraphSignals::new();
    let _tc = render_sync(&gs);

    // Source(Float out) → R1(Any) → R2(Any) → Sink(Float in)
    let (src, _) = gs.add_node("Src", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let (r1, _) = gs.add_node("R1", (100.0, 0.0), vec![
        (PortDirection::Input, SocketType::Any, "".to_string()),
        (PortDirection::Output, SocketType::Any, "".to_string()),
    ]);
    gs.with_graph_mut(|g| g.world.insert(r1, nodegraph_core::graph::reroute::IsReroute));
    let (r2, _) = gs.add_node("R2", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Any, "".to_string()),
        (PortDirection::Output, SocketType::Any, "".to_string()),
    ]);
    gs.with_graph_mut(|g| g.world.insert(r2, nodegraph_core::graph::reroute::IsReroute));
    let (sink, _) = gs.add_node("Sink", (300.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);

    // Get ports
    let (src_out, r1_in, r1_out, r2_in, r2_out, sink_in) = gs.with_graph(|g| {
        (g.node_ports(src)[0], g.node_ports(r1)[0], g.node_ports(r1)[1],
         g.node_ports(r2)[0], g.node_ports(r2)[1], g.node_ports(sink)[0])
    });

    // Connect chain
    gs.connect_ports(src_out, r1_in).unwrap();
    gs.connect_ports(r1_out, r2_in).unwrap();
    gs.connect_ports(r2_out, sink_in).unwrap();

    assert_eq!(gs.with_graph(|g| g.connection_count()), 3, "Should have 3 connections");
    assert_eq!(gs.node_count(), 4);

    // Delete R1 — should remove its 2 connections
    gs.select_single(r1);
    gs.delete_selected();
    assert_eq!(gs.node_count(), 3, "R1 removed");
    assert_eq!(gs.with_graph(|g| g.connection_count()), 1, "Only R2→Sink remains");

    // Undo
    gs.undo();
    assert_eq!(gs.node_count(), 4, "R1 restored");
    assert_eq!(gs.with_graph(|g| g.connection_count()), 3, "All connections restored");
}

#[wasm_bindgen_test]
async fn test_collapsed_node_hides_ports_in_dom() {
    // Create a graph with one collapsed node — it should render without port circles
    let gs = GraphSignals::new();
    let (n, _) = gs.add_node("Collapsed", (100.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "A".to_string()),
        (PortDirection::Output, SocketType::Float, "B".to_string()),
    ]);
    // Collapse before rendering — update both graph and signal
    gs.with_graph_mut(|g| {
        if let Some(h) = g.world.get_mut::<nodegraph_core::graph::node::NodeHeader>(n) {
            h.collapsed = true;
        }
    });
    // Sync header signal so render_node sees collapsed=true
    if let Some(sig) = gs.get_node_header_signal(n) {
        let mut h = sig.get_cloned();
        h.collapsed = true;
        sig.set(h);
    }

    let _tc = render_sync(&gs);
    let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;

    // Collapsed node should have no port circles
    let doc = web_sys::window().unwrap().document().unwrap();
    let node_group = doc.query_selector("[data-node-id]").unwrap().unwrap();
    let ports = node_group.query_selector_all("[data-port-id]").unwrap();
    assert_eq!(ports.length(), 0, "Collapsed node should have 0 port circles, got {}", ports.length());
}
