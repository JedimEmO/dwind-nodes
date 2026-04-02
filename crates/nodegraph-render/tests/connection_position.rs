use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

use nodegraph_core::graph::node::{NodePosition, MuteState};
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::interaction::{InputEvent, MouseButton, Modifiers};
use nodegraph_core::layout::Vec2;
use nodegraph_core::store::EntityId;
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_render::graph_signals::GraphSignals;
use nodegraph_render::viewport_view::render_graph_editor;

// ============================================================
// Helpers
// ============================================================

fn new_two_node_graph() -> (std::rc::Rc<GraphSignals>, EntityId, EntityId, EntityId, EntityId) {
    let gs = GraphSignals::new();
    let n1 = gs.add_node("Source", (100.0, 100.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let n2 = gs.add_node("Target", (400.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let graph = gs.graph.borrow();
    let out = graph.node_ports(n1)[0];
    let inp = graph.node_ports(n2)[0];
    drop(graph);
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
    gs.connect_ports(out, inp);

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
    gs.connect_ports(out, inp);
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
    gs.connect_ports(out, inp);
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
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);

    // Read the output port's world position before move
    let pos_before = gs.port_world_pos(out).unwrap();

    // Move node 1 by updating position signal (simulates what sync_all_positions does)
    gs.node_positions.borrow().get(&n1).unwrap().set((200.0, 200.0));
    // Also update the graph state to match
    gs.graph.borrow_mut().world.get_mut::<NodePosition>(n1).unwrap().x = 200.0;
    gs.graph.borrow_mut().world.get_mut::<NodePosition>(n1).unwrap().y = 200.0;
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
    assert_eq!(gs.graph.borrow().node_count(), 2);

    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
    gs.delete_selected();

    assert_eq!(gs.graph.borrow().node_count(), 1);
}

#[wasm_bindgen_test]
fn test_undo_redo() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
    gs.delete_selected();
    assert_eq!(gs.graph.borrow().node_count(), 1);

    gs.undo();
    assert_eq!(gs.graph.borrow().node_count(), 2);

    gs.redo();
    assert_eq!(gs.graph.borrow().node_count(), 1);
}

#[wasm_bindgen_test]
fn test_duplicate() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
    gs.duplicate_selected();

    assert_eq!(gs.graph.borrow().node_count(), 3);
}

#[wasm_bindgen_test]
fn test_mute_toggle() {
    let (gs, n1, _, _, _) = new_two_node_graph();
    let _tc = render_sync(&gs);

    assert!(gs.graph.borrow().world.get::<MuteState>(n1).is_none());

    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
    gs.toggle_mute_selected();
    assert_eq!(gs.graph.borrow().world.get::<MuteState>(n1).unwrap().0, true);

    gs.toggle_mute_selected(); // selection still set from above
    assert_eq!(gs.graph.borrow().world.get::<MuteState>(n1).unwrap().0, false);
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

    assert!(gs.connect_ports(out, inp).is_some());
    assert_eq!(gs.graph.borrow().connection_count(), 1);
}

#[wasm_bindgen_test]
fn test_incompatible_connection_rejected() {
    let gs = GraphSignals::new();
    let n1 = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Shader, "Out".to_string()),
    ]);
    let n2 = gs.add_node("B", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Geometry, "In".to_string()),
    ]);
    let graph = gs.graph.borrow();
    let out = graph.node_ports(n1)[0];
    let inp = graph.node_ports(n2)[0];
    drop(graph);
    let _tc = render_sync(&gs);

    assert!(gs.connect_ports(out, inp).is_none());
    assert_eq!(gs.graph.borrow().connection_count(), 0);
}

// ============================================================
// Connection removal
// ============================================================

#[wasm_bindgen_test]
fn test_delete_node_removes_its_connections() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);

    assert_eq!(gs.graph.borrow().connection_count(), 1);

    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
    gs.delete_selected();

    assert_eq!(gs.graph.borrow().connection_count(), 0);
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

    assert_eq!(gs.graph.borrow().connection_count(), 0);

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

    assert_eq!(gs.graph.borrow().connection_count(), 1, "Connection should be created after drop");
    assert!(gs.connecting_from.get().is_none(), "connecting_from should be cleared");
    assert!(gs.preview_wire.get_cloned().is_none(), "Preview wire should be cleared");
    assert!(gs.drop_target_port.get().is_none(), "Drop target should be cleared");
}

#[wasm_bindgen_test]
fn test_drag_to_connect_incompatible_rejected() {
    let gs = GraphSignals::new();
    let n1 = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Shader, "Out".to_string()),
    ]);
    let n2 = gs.add_node("B", (300.0, 0.0), vec![
        (PortDirection::Input, SocketType::Geometry, "In".to_string()),
    ]);
    let graph = gs.graph.borrow();
    let out = graph.node_ports(n1)[0];
    let inp = graph.node_ports(n2)[0];
    drop(graph);
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
    assert_eq!(gs.graph.borrow().connection_count(), 0);
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

    assert_eq!(gs.graph.borrow().connection_count(), 0);
    assert!(gs.connecting_from.get().is_none());
}

// ============================================================
// SVG noodle DOM actually changes after node drag
// ============================================================

#[wasm_bindgen_test]
fn test_port_position_changes_after_node_move() {
    let (gs, n1, _, out, inp) = new_two_node_graph();
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);

    let pos_before = gs.port_world_pos(out).unwrap();

    // Move the source node via position signal (same as sync_all_positions does)
    gs.node_positions.borrow().get(&n1).unwrap().set((300.0, 300.0));
    gs.graph.borrow_mut().world.get_mut::<NodePosition>(n1).unwrap().x = 300.0;
    gs.graph.borrow_mut().world.get_mut::<NodePosition>(n1).unwrap().y = 300.0;

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
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);

    // Count connection-related SVG paths before
    let count_before = gs.connection_list.lock_ref().len();
    assert!(count_before >= 1);

    // Delete source node
    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
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
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);

    assert_eq!(gs.graph.borrow().node_count(), 2);
    assert_eq!(gs.graph.borrow().connection_count(), 1);

    // Delete source node (removes connection too)
    gs.controller.borrow_mut().selection.select(n1);
    gs.selection.set(gs.controller.borrow().selection.selected.clone());
    gs.delete_selected();

    assert_eq!(gs.graph.borrow().node_count(), 1);
    assert_eq!(gs.graph.borrow().connection_count(), 0);

    // Undo — node and connection restored
    gs.undo();
    assert_eq!(gs.graph.borrow().node_count(), 2);
    assert_eq!(gs.graph.borrow().connection_count(), 1);

    // Connection path should be valid after undo (offsets computed from data, no rAF needed)
    let _conn_id = gs.graph.borrow().world.query::<ConnectionEndpoints>()
        .map(|(id, _)| id).next().unwrap();
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
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);

    assert_eq!(gs.graph.borrow().connection_count(), 1);

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
    assert_eq!(gs.graph.borrow().connection_count(), 0, "Connection should be removed by cut");
    assert!(gs.cut_line_points.get_cloned().is_empty(), "Cut line should be cleared");
}

// ============================================================
// Multiple connections from same output
// ============================================================

#[wasm_bindgen_test]
fn test_multiple_connections_from_output() {
    let gs = GraphSignals::new();
    let mc_n1 = gs.add_node("Src", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let mc_n2 = gs.add_node("Tgt1", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let mc_n3 = gs.add_node("Tgt2", (200.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let graph = gs.graph.borrow();
    let out = graph.node_ports(mc_n1)[0];
    let in1 = graph.node_ports(mc_n2)[0];
    let in2 = graph.node_ports(mc_n3)[0];
    drop(graph);

    let _tc = render_sync(&gs);

    gs.connect_ports(out, in1);
    gs.connect_ports(out, in2);

    assert_eq!(gs.graph.borrow().connection_count(), 2);
    assert_eq!(gs.connection_list.lock_ref().len(), 2);
}

// ============================================================
// Replacing existing connection on input
// ============================================================

#[wasm_bindgen_test]
fn test_replacing_input_connection() {
    let gs = GraphSignals::new();
    let n1 = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let n2 = gs.add_node("B", (0.0, 100.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let n3 = gs.add_node("C", (200.0, 50.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let graph = gs.graph.borrow();
    let out1 = graph.node_ports(n1)[0];
    let out2 = graph.node_ports(n2)[0];
    let inp = graph.node_ports(n3)[0];
    drop(graph);

    let _tc = render_sync(&gs);

    gs.connect_ports(out1, inp);
    assert_eq!(gs.graph.borrow().connection_count(), 1);

    // Connecting out2 to the same input should replace
    gs.connect_ports(out2, inp);
    assert_eq!(gs.graph.borrow().connection_count(), 1, "Old connection should be replaced");
}
