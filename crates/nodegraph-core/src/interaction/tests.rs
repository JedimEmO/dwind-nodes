use super::*;
use crate::graph::node::NodePosition;
use crate::graph::port::PortDirection;
use crate::graph::NodeGraph;
use crate::layout::{
    self, compute_node_layout, compute_port_world_position, LayoutCache, Rect, Vec2,
};
use crate::layout::{HEADER_HEIGHT, NODE_MIN_WIDTH, PORT_HEIGHT};
use crate::types::socket_type::SocketType;
use crate::viewport::Viewport;

fn make_test_graph() -> NodeGraph {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("Node 1", (0.0, 0.0));
    graph.add_port(n1, PortDirection::Input, SocketType::Float, "A");
    graph.add_port(n1, PortDirection::Input, SocketType::Float, "B");
    graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");

    let n2 = graph.add_node("Node 2", (300.0, 0.0));
    graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    graph.add_port(n2, PortDirection::Output, SocketType::Float, "X");
    graph.add_port(n2, PortDirection::Output, SocketType::Float, "Y");

    let n3 = graph.add_node("Node 3", (600.0, 200.0));
    graph.add_port(n3, PortDirection::Input, SocketType::Float, "In");
    graph.add_port(n3, PortDirection::Output, SocketType::Float, "Out");

    graph
}

// ============================================================
// AC 1: Viewport transforms
// ============================================================

#[test]
fn viewport_screen_to_world() {
    let vp = Viewport {
        pan: (100.0, 50.0),
        zoom: 2.0,
    };
    let (wx, wy) = vp.screen_to_world(0.0, 0.0);
    assert!((wx - (-50.0)).abs() < 1e-10);
    assert!((wy - (-25.0)).abs() < 1e-10);
}

#[test]
fn viewport_world_to_screen() {
    let vp = Viewport {
        pan: (100.0, 50.0),
        zoom: 2.0,
    };
    let (sx, sy) = vp.world_to_screen(0.0, 0.0);
    assert!((sx - 100.0).abs() < 1e-10);
    assert!((sy - 50.0).abs() < 1e-10);
}

#[test]
fn viewport_roundtrip() {
    let vp = Viewport {
        pan: (37.0, -12.0),
        zoom: 1.5,
    };
    let (sx, sy) = vp.world_to_screen(100.0, 200.0);
    let (wx, wy) = vp.screen_to_world(sx, sy);
    assert!((wx - 100.0).abs() < 1e-10);
    assert!((wy - 200.0).abs() < 1e-10);
}

#[test]
fn viewport_zoom_at_preserves_fixed_point() {
    let mut vp = Viewport {
        pan: (100.0, 50.0),
        zoom: 1.0,
    };
    let screen_x = 400.0;
    let screen_y = 300.0;

    let (wx_before, wy_before) = vp.screen_to_world(screen_x, screen_y);
    vp.zoom_at(screen_x, screen_y, 2.0);
    let (wx_after, wy_after) = vp.screen_to_world(screen_x, screen_y);

    assert!((wx_before - wx_after).abs() < 1e-10);
    assert!((wy_before - wy_after).abs() < 1e-10);
    assert!((vp.zoom - 2.0).abs() < 1e-10);
}

#[test]
fn viewport_fit_to_bounds() {
    let mut vp = Viewport::new();
    vp.fit_to_bounds((100.0, 100.0, 200.0, 200.0), (800.0, 600.0));

    // The center of the bounds (200, 200) should map to the center of the viewport (400, 300)
    let (sx, sy) = vp.world_to_screen(200.0, 200.0);
    assert!((sx - 400.0).abs() < 1e-10);
    assert!((sy - 300.0).abs() < 1e-10);
}

// ============================================================
// AC 2: Node layout
// ============================================================

#[test]
fn node_layout_basic() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("Test", (100.0, 50.0));
    graph.add_port(node, PortDirection::Input, SocketType::Float, "In 1");
    graph.add_port(node, PortDirection::Input, SocketType::Float, "In 2");
    graph.add_port(node, PortDirection::Input, SocketType::Float, "In 3");
    graph.add_port(node, PortDirection::Output, SocketType::Float, "Out 1");
    graph.add_port(node, PortDirection::Output, SocketType::Float, "Out 2");

    let layout = compute_node_layout(&graph, node).unwrap();

    // Header
    assert_eq!(layout.header_rect.x, 100.0);
    assert_eq!(layout.header_rect.y, 50.0);
    assert_eq!(layout.header_rect.w, NODE_MIN_WIDTH);
    assert_eq!(layout.header_rect.h, HEADER_HEIGHT);

    // Body height = max(3 inputs, 2 outputs) * PORT_HEIGHT = 3 * PORT_HEIGHT
    let expected_body_h = 3.0 * PORT_HEIGHT;
    assert!((layout.body_rect.h - expected_body_h).abs() < 1e-10);

    // Total
    let expected_total_h = HEADER_HEIGHT + expected_body_h;
    assert!((layout.total_rect.h - expected_total_h).abs() < 1e-10);

    // Port positions
    assert_eq!(layout.input_port_positions.len(), 3);
    assert_eq!(layout.output_port_positions.len(), 2);

    // Input ports on left edge (x = node.x)
    for (_, pos) in &layout.input_port_positions {
        assert!((pos.x - 100.0).abs() < 1e-10);
    }
    // Output ports on right edge (x = node.x + width)
    for (_, pos) in &layout.output_port_positions {
        assert!((pos.x - (100.0 + NODE_MIN_WIDTH)).abs() < 1e-10);
    }

    // Ports are vertically spaced
    let y_vals: Vec<f64> = layout
        .input_port_positions
        .iter()
        .map(|(_, p)| p.y)
        .collect();
    assert!(y_vals[1] > y_vals[0]);
    assert!(y_vals[2] > y_vals[1]);
    assert!((y_vals[1] - y_vals[0] - PORT_HEIGHT).abs() < 1e-10);
}

#[test]
fn node_layout_collapsed() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("Test", (0.0, 0.0));
    graph.add_port(node, PortDirection::Input, SocketType::Float, "In");
    graph.add_port(node, PortDirection::Output, SocketType::Float, "Out");

    // Collapse the node
    if let Some(header) = graph.world.get_mut::<crate::graph::node::NodeHeader>(node) {
        header.collapsed = true;
    }

    let layout = compute_node_layout(&graph, node).unwrap();

    // Body height should be 0, total = just header
    assert!((layout.body_rect.h - 0.0).abs() < 1e-10);
    assert!((layout.total_rect.h - HEADER_HEIGHT).abs() < 1e-10);

    // No port positions when collapsed
    assert_eq!(layout.input_port_positions.len(), 0);
    assert_eq!(layout.output_port_positions.len(), 0);
}

// ============================================================
// AC 3: Bezier path
// ============================================================

#[test]
fn bezier_path_svg() {
    let path = layout::compute_connection_path(Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0));

    let d = path.to_svg_d();
    assert!(d.starts_with("M 0 0 C "));
    assert!(d.contains("200 100"));

    // Control points should extend horizontally
    // cp1.x > start.x (extends right from output)
    assert!(path.cp1.x > path.start.x);
    // cp2.x < end.x (extends left into input)
    assert!(path.cp2.x < path.end.x);
    // cp1.y == start.y (horizontal extension)
    assert!((path.cp1.y - path.start.y).abs() < 1e-10);
    // cp2.y == end.y
    assert!((path.cp2.y - path.end.y).abs() < 1e-10);
}

#[test]
fn bezier_path_point_at_endpoints() {
    let path = layout::compute_connection_path(Vec2::new(10.0, 20.0), Vec2::new(200.0, 100.0));

    let p0 = path.point_at(0.0);
    assert!((p0.x - 10.0).abs() < 1e-10);
    assert!((p0.y - 20.0).abs() < 1e-10);

    let p1 = path.point_at(1.0);
    assert!((p1.x - 200.0).abs() < 1e-10);
    assert!((p1.y - 100.0).abs() < 1e-10);
}

#[test]
fn bezier_distance_to_point_on_curve() {
    let path = layout::compute_connection_path(Vec2::new(0.0, 0.0), Vec2::new(200.0, 0.0));
    // A point on the curve at t=0.5 should have ~0 distance
    let midpoint = path.point_at(0.5);
    let dist = path.distance_to_point(midpoint);
    assert!(dist < 2.0); // sampling approximation has some error
}

// ============================================================
// AC 4: Hit testing
// ============================================================

#[test]
fn hit_test_node() {
    let graph = make_test_graph();
    let cache = LayoutCache::compute(&graph);
    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();

    // Find "Node 1" (at 0,0)
    let node1 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 0.0)
        .unwrap();

    // Click inside node 1's header area
    let result = hit_test(&graph, &cache, Vec2::new(80.0, 10.0));
    assert_eq!(result, HitTarget::Node(*node1));

    // Click on empty space far away
    let result = hit_test(&graph, &cache, Vec2::new(1000.0, 1000.0));
    assert_eq!(result, HitTarget::Nothing);
}

#[test]
fn hit_test_port() {
    let graph = make_test_graph();
    let cache = LayoutCache::compute(&graph);

    // Get node 1's output port position
    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();
    let node1 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 0.0)
        .unwrap();

    let layout = cache.node_layout(*node1).unwrap();
    let (out_port_id, out_port_pos) = layout.output_port_positions[0];

    // Click right on the port center
    let result = hit_test(&graph, &cache, out_port_pos);
    assert_eq!(result, HitTarget::Port(out_port_id));
}

#[test]
fn hit_test_rect_selects_contained_nodes() {
    let graph = make_test_graph();
    let cache = LayoutCache::compute(&graph);

    // A rect that covers nodes at (0,0) and (300,0) but not (600,200)
    let rect = Rect::new(-10.0, -10.0, 500.0, 200.0);
    let hits = hit_test_rect(&cache, rect);
    assert_eq!(hits.len(), 2);

    // A rect that covers everything
    let rect = Rect::new(-10.0, -10.0, 800.0, 400.0);
    let hits = hit_test_rect(&cache, rect);
    assert_eq!(hits.len(), 3);

    // A rect that covers nothing
    let rect = Rect::new(2000.0, 2000.0, 10.0, 10.0);
    let hits = hit_test_rect(&cache, rect);
    assert_eq!(hits.len(), 0);
}

// ============================================================
// AC 5: Drag state machine
// ============================================================

#[test]
fn drag_node_moves_position() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();
    let node1 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 0.0)
        .copied()
        .unwrap();

    let orig_pos = graph.world.get::<NodePosition>(node1).unwrap().clone();

    // Click on node1 header area (center of header)
    let click_pos = Vec2::new(80.0, 10.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: click_pos,
            world: click_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::DraggingNodes { .. }));

    // Move by (50, 30)
    let move_pos = Vec2::new(130.0, 40.0);
    ctrl.handle_event(
        InputEvent::MouseMove {
            screen: move_pos,
            world: move_pos,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    // Release
    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: move_pos,
            world: move_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));

    let new_pos = graph.world.get::<NodePosition>(node1).unwrap();
    assert!((new_pos.x - (orig_pos.x + 50.0)).abs() < 1e-10);
    assert!((new_pos.y - (orig_pos.y + 30.0)).abs() < 1e-10);
}

#[test]
fn pan_with_left_mouse_on_empty_canvas() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    // World coord far from all test-graph nodes (at (0,0), (300,0), (600,200)).
    let start = Vec2::new(1000.0, 1000.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: start,
            world: start,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Panning { .. }));

    let moved = Vec2::new(1050.0, 1020.0);
    ctrl.handle_event(
        InputEvent::MouseMove {
            screen: moved,
            world: moved,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!((ctrl.viewport.pan.0 - 50.0).abs() < 1e-10);
    assert!((ctrl.viewport.pan.1 - 20.0).abs() < 1e-10);

    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: moved,
            world: moved,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));
}

#[test]
fn shift_left_mouse_on_empty_canvas_box_selects() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    let start = Vec2::new(1000.0, 1000.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: start,
            world: start,
            button: MouseButton::Left,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::BoxSelecting { .. }));
}

// ============================================================
// AC 6: Connection drag
// ============================================================

#[test]
fn connection_drag_creates_connection() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (300.0, 0.0));
    let inp = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");

    let mut ctrl = InteractionController::new();

    // Get port positions
    let out_pos = compute_port_world_position(&graph, out).unwrap();
    let in_pos = compute_port_world_position(&graph, inp).unwrap();

    // Mousedown on output port
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: out_pos,
            world: out_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(
        ctrl.state,
        InteractionState::ConnectingPort { .. }
    ));

    // Move toward input
    let mid = Vec2::new(150.0, 0.0);
    let effects = ctrl.handle_event(
        InputEvent::MouseMove {
            screen: mid,
            world: mid,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    // Should produce a preview wire effect
    assert!(effects
        .iter()
        .any(|e| matches!(e, SideEffect::PreviewWire { .. })));

    // Release on input port
    let effects = ctrl.handle_event(
        InputEvent::MouseUp {
            screen: in_pos,
            world: in_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));
    assert!(effects
        .iter()
        .any(|e| matches!(e, SideEffect::ConnectionCreated(_))));
    assert_eq!(graph.connection_count(), 1);
}

#[test]
fn connection_drag_incompatible_fails() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (300.0, 0.0));
    let inp = graph.add_port(n2, PortDirection::Input, SocketType::Shader, "In");

    let mut ctrl = InteractionController::new();
    let out_pos = compute_port_world_position(&graph, out).unwrap();
    let in_pos = compute_port_world_position(&graph, inp).unwrap();

    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: out_pos,
            world: out_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    let effects = ctrl.handle_event(
        InputEvent::MouseUp {
            screen: in_pos,
            world: in_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));
    assert!(effects
        .iter()
        .any(|e| matches!(e, SideEffect::ConnectionFailed)));
    assert_eq!(graph.connection_count(), 0);
}

#[test]
fn connection_drag_release_on_empty_cancels() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");

    let mut ctrl = InteractionController::new();
    let out_pos = compute_port_world_position(&graph, out).unwrap();

    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: out_pos,
            world: out_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    let far_away = Vec2::new(999.0, 999.0);
    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: far_away,
            world: far_away,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));
    assert_eq!(graph.connection_count(), 0);
}

// ============================================================
// AC 7: Box selection
// ============================================================

#[test]
fn box_selection() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    // Shift+click on empty space starts box select (plain LMB on empty canvas pans).
    let start = Vec2::new(-20.0, -20.0);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: start,
            world: start,
            button: MouseButton::Left,
            modifiers: shift,
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::BoxSelecting { .. }));

    // Drag to cover first two nodes (at 0,0 and 300,0) but not third (600,200)
    let end = Vec2::new(500.0, 150.0);
    let effects = ctrl.handle_event(
        InputEvent::MouseMove {
            screen: end,
            world: end,
            modifiers: shift,
        },
        &mut graph,
    );

    assert!(effects
        .iter()
        .any(|e| matches!(e, SideEffect::BoxSelectRect { .. })));

    // Release
    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: end,
            world: end,
            button: MouseButton::Left,
            modifiers: shift,
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));
    assert_eq!(ctrl.selection.selected.len(), 2);
}

#[test]
fn box_selection_shift_adds() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    // First select nodes at (0,0) and (300,0) via shift-box-select.
    let start = Vec2::new(-20.0, -20.0);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: start,
            world: start,
            button: MouseButton::Left,
            modifiers: shift,
        },
        &mut graph,
    );
    let end = Vec2::new(500.0, 150.0);
    ctrl.handle_event(
        InputEvent::MouseMove {
            screen: end,
            world: end,
            modifiers: shift,
        },
        &mut graph,
    );
    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: end,
            world: end,
            button: MouseButton::Left,
            modifiers: shift,
        },
        &mut graph,
    );
    assert_eq!(ctrl.selection.selected.len(), 2);

    // Now shift-box-select to add node at (600,200)
    let start2 = Vec2::new(550.0, 150.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: start2,
            world: start2,
            button: MouseButton::Left,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        },
        &mut graph,
    );
    let end2 = Vec2::new(800.0, 400.0);
    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: end2,
            world: end2,
            button: MouseButton::Left,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        },
        &mut graph,
    );

    assert_eq!(ctrl.selection.selected.len(), 3);
}

// ============================================================
// Additional edge cases
// ============================================================

#[test]
fn scroll_changes_zoom() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    let original_zoom = ctrl.viewport.zoom;
    ctrl.handle_event(
        InputEvent::Scroll {
            screen: Vec2::new(400.0, 300.0),
            delta: 1.0,
        },
        &mut graph,
    );

    assert!(ctrl.viewport.zoom > original_zoom);
}

#[test]
fn rect_from_corners() {
    let r = Rect::from_corners(Vec2::new(10.0, 20.0), Vec2::new(5.0, 30.0));
    assert_eq!(r.x, 5.0);
    assert_eq!(r.y, 20.0);
    assert_eq!(r.w, 5.0);
    assert_eq!(r.h, 10.0);
}

#[test]
fn rect_contains_and_intersects() {
    let r = Rect::new(10.0, 10.0, 100.0, 100.0);
    assert!(r.contains(Vec2::new(50.0, 50.0)));
    assert!(!r.contains(Vec2::new(5.0, 5.0)));

    let r2 = Rect::new(50.0, 50.0, 200.0, 200.0);
    assert!(r.intersects(&r2));

    let r3 = Rect::new(200.0, 200.0, 10.0, 10.0);
    assert!(!r.intersects(&r3));
}

#[test]
fn selection_toggle() {
    let mut sel = SelectionState::new();
    let id = crate::store::EntityId {
        index: 0,
        generation: crate::store::Generation::default(),
    };

    sel.toggle(id);
    assert!(sel.is_selected(id));
    sel.toggle(id);
    assert!(!sel.is_selected(id));
}

#[test]
fn layout_cache_computes_all() {
    let mut graph = make_test_graph();

    // Add a connection so we can test connection paths
    let ports: Vec<_> = graph
        .world
        .query::<PortDirection>()
        .map(|(id, dir)| (id, *dir))
        .collect();
    let out = ports
        .iter()
        .find(|(id, dir)| {
            *dir == PortDirection::Output
                && graph
                    .world
                    .get::<crate::graph::port::PortOwner>(*id)
                    .map(|o| graph.world.get::<NodePosition>(o.0).unwrap().x == 0.0)
                    .unwrap_or(false)
        })
        .unwrap()
        .0;
    let inp = ports
        .iter()
        .find(|(id, dir)| {
            *dir == PortDirection::Input
                && graph
                    .world
                    .get::<crate::graph::port::PortOwner>(*id)
                    .map(|o| graph.world.get::<NodePosition>(o.0).unwrap().x == 300.0)
                    .unwrap_or(false)
        })
        .unwrap()
        .0;
    graph.connect(out, inp).unwrap();

    let cache = LayoutCache::compute(&graph);
    assert_eq!(cache.layouts.len(), 3);
    assert_eq!(cache.connection_paths.len(), 1);
}

// ============================================================
// Connection wire hit testing
// ============================================================

fn make_connected_graph() -> (NodeGraph, EntityId, EntityId) {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (300.0, 0.0));
    let inp = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    let conn = graph.connect(out, inp).unwrap();
    (graph, conn, out)
}

#[test]
fn hit_test_connection_wire() {
    let (graph, conn_id, _) = make_connected_graph();
    let cache = LayoutCache::compute(&graph);

    // The connection goes from ~(160, ~39) to ~(300, ~39)
    // The midpoint of the bezier should be hittable
    let path = cache.connection_paths.get(&conn_id).unwrap();
    let midpoint = path.point_at(0.5);

    let result = hit_test(&graph, &cache, midpoint);
    assert_eq!(result, HitTarget::Connection(conn_id));
}

// ============================================================
// Cutting links
// ============================================================

#[test]
fn ctrl_rmb_starts_cutting_mode() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    let start = Vec2::new(100.0, 100.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: start,
            world: start,
            button: MouseButton::Right,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::CuttingLinks { .. }));
}

#[test]
fn cutting_links_disconnects_intersected() {
    let (mut graph, _conn_id, _) = make_connected_graph();
    assert_eq!(graph.connection_count(), 1);

    let mut ctrl = InteractionController::new();
    let cache = LayoutCache::compute(&graph);

    // Get the connection path so we can cut across it
    let path = cache.connection_paths.values().next().unwrap();
    let wire_mid = path.point_at(0.5);

    // Start cutting above the wire
    let above = Vec2::new(wire_mid.x, wire_mid.y - 50.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: above,
            world: above,
            button: MouseButton::Right,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        &mut graph,
    );

    // Move below the wire (crossing it)
    let below = Vec2::new(wire_mid.x, wire_mid.y + 50.0);
    ctrl.handle_event(
        InputEvent::MouseMove {
            screen: below,
            world: below,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        &mut graph,
    );

    // Release
    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: below,
            world: below,
            button: MouseButton::Right,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        },
        &mut graph,
    );

    assert!(matches!(ctrl.state, InteractionState::Idle));
    assert_eq!(graph.connection_count(), 0);
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn node_layout_zero_ports() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("Empty", (50.0, 50.0));

    let layout = compute_node_layout(&graph, node).unwrap();
    assert!((layout.body_rect.h - 0.0).abs() < 1e-10);
    assert!((layout.total_rect.h - HEADER_HEIGHT).abs() < 1e-10);
    assert_eq!(layout.input_port_positions.len(), 0);
    assert_eq!(layout.output_port_positions.len(), 0);
}

#[test]
fn node_layout_inputs_only() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("Sink", (0.0, 0.0));
    graph.add_port(node, PortDirection::Input, SocketType::Float, "A");
    graph.add_port(node, PortDirection::Input, SocketType::Float, "B");

    let layout = compute_node_layout(&graph, node).unwrap();
    assert_eq!(layout.input_port_positions.len(), 2);
    assert_eq!(layout.output_port_positions.len(), 0);
    assert!((layout.body_rect.h - 2.0 * PORT_HEIGHT).abs() < 1e-10);
}

#[test]
fn viewport_zoom_clamp_min() {
    let mut vp = Viewport::new();
    vp.zoom_at(0.0, 0.0, 0.01); // below minimum 0.1
    assert!((vp.zoom - 0.1).abs() < 1e-10);
}

#[test]
fn viewport_zoom_clamp_max() {
    let mut vp = Viewport::new();
    vp.zoom_at(0.0, 0.0, 20.0); // above maximum 10.0
    assert!((vp.zoom - 10.0).abs() < 1e-10);
}

#[test]
fn drag_multiple_selected_nodes() {
    let mut graph = make_test_graph();
    let mut ctrl = InteractionController::new();

    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();
    let node1 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 0.0)
        .copied()
        .unwrap();
    let node2 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 300.0)
        .copied()
        .unwrap();

    // Pre-select both nodes
    ctrl.selection.select(node1);
    ctrl.selection.select(node2);

    let orig_pos1 = graph.world.get::<NodePosition>(node1).unwrap().clone();
    let orig_pos2 = graph.world.get::<NodePosition>(node2).unwrap().clone();

    // Click on node1 header (already selected, so it starts dragging both)
    let click = Vec2::new(80.0, 10.0);
    ctrl.handle_event(
        InputEvent::MouseDown {
            screen: click,
            world: click,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    // Move by (25, 15)
    let moved = Vec2::new(105.0, 25.0);
    ctrl.handle_event(
        InputEvent::MouseMove {
            screen: moved,
            world: moved,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: moved,
            world: moved,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    let new1 = graph.world.get::<NodePosition>(node1).unwrap();
    let new2 = graph.world.get::<NodePosition>(node2).unwrap();
    assert!((new1.x - (orig_pos1.x + 25.0)).abs() < 1e-10);
    assert!((new1.y - (orig_pos1.y + 15.0)).abs() < 1e-10);
    assert!((new2.x - (orig_pos2.x + 25.0)).abs() < 1e-10);
    assert!((new2.y - (orig_pos2.y + 15.0)).abs() < 1e-10);
}

#[test]
fn segments_intersect_basic() {
    // Crossing X
    assert!(super::segments_intersect(
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(0.0, 10.0),
        Vec2::new(10.0, 0.0),
    ));

    // Parallel lines
    assert!(!super::segments_intersect(
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(0.0, 5.0),
        Vec2::new(10.0, 5.0),
    ));

    // Non-intersecting
    assert!(!super::segments_intersect(
        Vec2::new(0.0, 0.0),
        Vec2::new(5.0, 0.0),
        Vec2::new(6.0, -1.0),
        Vec2::new(6.0, 1.0),
    ));
}

// ============================================================
// Frame hit testing and dragging
// ============================================================

#[test]
fn hit_test_frame_detects_click_inside_frame() {
    let mut graph = make_test_graph();
    // Add a frame around all nodes
    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();
    graph.add_frame("Test Frame", [80, 80, 120], &nodes);

    let cache = LayoutCache::compute(&graph);
    // Click inside the frame padding area (not on a node)
    // Nodes are at (0,0) and (300,50), frame extends with 30px padding
    // So frame goes from roughly (-30, -30) to (460+30, 50+height+30)
    let target = hit_test(&graph, &cache, Vec2::new(-20.0, -20.0));
    assert!(
        matches!(target, HitTarget::Frame(_)),
        "Click in frame padding should hit frame"
    );
}

#[test]
fn hit_test_node_over_frame() {
    let mut graph = make_test_graph();
    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();
    graph.add_frame("Test Frame", [80, 80, 120], &nodes);

    let cache = LayoutCache::compute(&graph);
    // Click on the center of node 1 (at 0,0, width=160, header=28)
    let target = hit_test(&graph, &cache, Vec2::new(80.0, 14.0));
    assert!(
        matches!(target, HitTarget::Node(_)),
        "Click on a node should hit node, not frame"
    );
}

#[test]
fn drag_frame_moves_all_member_nodes() {
    let mut graph = make_test_graph();
    let nodes: Vec<EntityId> = graph
        .world
        .query::<crate::graph::node::NodeHeader>()
        .map(|(id, _)| id)
        .collect();
    // Frame only the first two nodes (n1 and n2)
    let n1 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 0.0)
        .copied()
        .unwrap();
    let n2 = nodes
        .iter()
        .find(|&&id| graph.world.get::<NodePosition>(id).unwrap().x == 300.0)
        .copied()
        .unwrap();
    let frame_members = vec![n1, n2];

    let orig_n1 = graph.world.get::<NodePosition>(n1).unwrap().clone();
    let orig_n2 = graph.world.get::<NodePosition>(n2).unwrap().clone();

    graph.add_frame("Test Frame", [80, 80, 120], &frame_members);

    let mut ctrl = InteractionController::new();

    // Click inside frame padding (not on any node)
    // Frame spans from about (-30, -30) to (460+30, height+30)
    let click = Vec2::new(-20.0, -20.0);
    let effects = ctrl.handle_event(
        InputEvent::MouseDown {
            screen: click,
            world: click,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    assert!(
        matches!(ctrl.state, InteractionState::DraggingNodes { .. }),
        "Should start dragging"
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, SideEffect::SelectionChanged)),
        "Should select members"
    );
    assert_eq!(
        ctrl.selection.selected.len(),
        2,
        "Both frame member nodes should be selected"
    );

    // Drag by (100, 50)
    let move_to = Vec2::new(80.0, 30.0);
    ctrl.handle_event(
        InputEvent::MouseMove {
            screen: move_to,
            world: move_to,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    ctrl.handle_event(
        InputEvent::MouseUp {
            screen: move_to,
            world: move_to,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        },
        &mut graph,
    );

    let new_n1 = graph.world.get::<NodePosition>(n1).unwrap();
    let new_n2 = graph.world.get::<NodePosition>(n2).unwrap();
    assert!(
        (new_n1.x - (orig_n1.x + 100.0)).abs() < 1e-10,
        "N1 x should move by 100"
    );
    assert!(
        (new_n1.y - (orig_n1.y + 50.0)).abs() < 1e-10,
        "N1 y should move by 50"
    );
    assert!(
        (new_n2.x - (orig_n2.x + 100.0)).abs() < 1e-10,
        "N2 x should move by 100"
    );
    assert!(
        (new_n2.y - (orig_n2.y + 50.0)).abs() < 1e-10,
        "N2 y should move by 50"
    );
}
