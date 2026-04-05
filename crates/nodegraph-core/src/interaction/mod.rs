use crate::graph::NodeGraph;
use crate::graph::node::NodePosition;
use crate::graph::port::PortDirection;
use crate::layout::{
    self, BezierPath, LayoutCache, Rect, Vec2,
    PORT_RADIUS, compute_port_world_position,
};
use crate::store::EntityId;
use crate::viewport::Viewport;

// ============================================================
// Hit testing
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitTarget {
    Nothing,
    Node(EntityId),
    Port(EntityId),
    Connection(EntityId),
    Frame(EntityId),
}

const CONNECTION_HIT_THRESHOLD: f64 = 8.0;

/// Hit test a world-space point against the graph.
/// Priority: ports > nodes > connections > frames.
pub fn hit_test(_graph: &NodeGraph, cache: &LayoutCache, world_pos: Vec2) -> HitTarget {
    // Check ports first (highest priority, smallest targets)
    for (_, layout) in &cache.layouts {
        for &(port_id, pos) in layout.input_port_positions.iter().chain(layout.output_port_positions.iter()) {
            if world_pos.distance_to(pos) <= PORT_RADIUS {
                return HitTarget::Port(port_id);
            }
        }
    }

    // Check node rects
    for (&node_id, layout) in &cache.layouts {
        if layout.total_rect.contains(world_pos) {
            return HitTarget::Node(node_id);
        }
    }

    // Check connections
    for (&conn_id, path) in &cache.connection_paths {
        if path.distance_to_point(world_pos) <= CONNECTION_HIT_THRESHOLD {
            return HitTarget::Connection(conn_id);
        }
    }

    // Check frames (lowest priority — behind everything else)
    for (&frame_id, (rect, _)) in &cache.frame_rects {
        if rect.contains(world_pos) {
            return HitTarget::Frame(frame_id);
        }
    }

    HitTarget::Nothing
}

/// Hit test connections only — ignores nodes, ports, frames.
/// Used when checking if a dragged node landed on a wire.
pub fn hit_test_connection(cache: &LayoutCache, world_pos: Vec2) -> Option<EntityId> {
    for (&conn_id, path) in &cache.connection_paths {
        if path.distance_to_point(world_pos) <= CONNECTION_HIT_THRESHOLD {
            return Some(conn_id);
        }
    }
    None
}

/// Find all nodes whose bounding box intersects the given world-space rect.
pub fn hit_test_rect(cache: &LayoutCache, world_rect: Rect) -> Vec<EntityId> {
    cache
        .layouts
        .iter()
        .filter(|(_, layout)| layout.total_rect.intersects(&world_rect))
        .map(|(&node_id, _)| node_id)
        .collect()
}

// ============================================================
// Input abstraction
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    MouseDown {
        screen: Vec2,
        world: Vec2,
        button: MouseButton,
        modifiers: Modifiers,
    },
    MouseMove {
        screen: Vec2,
        world: Vec2,
        modifiers: Modifiers,
    },
    MouseUp {
        screen: Vec2,
        world: Vec2,
        button: MouseButton,
        modifiers: Modifiers,
    },
    Scroll {
        screen: Vec2,
        delta: f64,
    },
}

// ============================================================
// Side effects (rendering hints)
// ============================================================

#[derive(Clone, Debug)]
pub enum SideEffect {
    PreviewWire { path: BezierPath },
    BoxSelectRect { rect: Rect },
    ClearTransient,
    SelectionChanged,
    NodesMoved,
    ConnectionCreated(EntityId),
    ConnectionFailed,
    FrameSelected(EntityId),
    FrameDeselected,
}

// ============================================================
// Selection state
// ============================================================

#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    pub selected: Vec<EntityId>,
}

impl SelectionState {
    pub fn new() -> Self {
        Self { selected: Vec::new() }
    }

    pub fn is_selected(&self, id: EntityId) -> bool {
        self.selected.contains(&id)
    }

    pub fn select(&mut self, id: EntityId) {
        if !self.selected.contains(&id) {
            self.selected.push(id);
        }
    }

    pub fn deselect(&mut self, id: EntityId) {
        self.selected.retain(|&e| e != id);
    }

    pub fn toggle(&mut self, id: EntityId) {
        if self.is_selected(id) {
            self.deselect(id);
        } else {
            self.select(id);
        }
    }

    pub fn clear(&mut self) {
        self.selected.clear();
    }

    pub fn set(&mut self, ids: Vec<EntityId>) {
        self.selected = ids;
    }
}

// ============================================================
// Interaction state machine
// ============================================================

#[derive(Clone, Debug)]
pub enum InteractionState {
    Idle,
    Panning {
        last_screen: Vec2,
    },
    DraggingNodes {
        node_ids: Vec<EntityId>,
        start_positions: Vec<(f64, f64)>,
        last_world: Vec2,
    },
    ConnectingPort {
        source_port: EntityId,
        from_output: bool,
        cursor_world: Vec2,
    },
    BoxSelecting {
        start_world: Vec2,
        current_world: Vec2,
    },
    CuttingLinks {
        points: Vec<Vec2>,
    },
}

pub struct InteractionController {
    pub state: InteractionState,
    pub selection: SelectionState,
    pub viewport: Viewport,
}

impl InteractionController {
    pub fn new() -> Self {
        Self {
            state: InteractionState::Idle,
            selection: SelectionState::new(),
            viewport: Viewport::new(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: InputEvent,
        graph: &mut NodeGraph,
    ) -> Vec<SideEffect> {
        let cache = LayoutCache::compute(graph);
        let mut effects = Vec::new();

        match self.state.clone() {
            InteractionState::Idle => {
                self.handle_idle(event, graph, &cache, &mut effects);
            }
            InteractionState::Panning { last_screen } => {
                self.handle_panning(event, last_screen, &mut effects);
            }
            InteractionState::DraggingNodes { node_ids, start_positions, last_world } => {
                self.handle_dragging(event, graph, node_ids, start_positions, last_world, &mut effects);
            }
            InteractionState::ConnectingPort { source_port, from_output, .. } => {
                self.handle_connecting(event, graph, &cache, source_port, from_output, &mut effects);
            }
            InteractionState::BoxSelecting { start_world, .. } => {
                self.handle_box_selecting(event, graph, &cache, start_world, &mut effects);
            }
            InteractionState::CuttingLinks { points } => {
                self.handle_cutting(event, graph, &cache, points, &mut effects);
            }
        }

        effects
    }

    fn handle_idle(
        &mut self,
        event: InputEvent,
        graph: &mut NodeGraph,
        cache: &LayoutCache,
        effects: &mut Vec<SideEffect>,
    ) {
        match event {
            InputEvent::MouseDown { world, button: MouseButton::Left, modifiers, .. } => {
                let target = hit_test(graph, cache, world);
                // Deselect frames when clicking on non-frame targets
                if !matches!(target, HitTarget::Frame(_)) {
                    effects.push(SideEffect::FrameDeselected);
                }

                match target {
                    HitTarget::Port(port_id) => {
                        let from_output = graph
                            .world
                            .get::<PortDirection>(port_id)
                            .map(|d| *d == PortDirection::Output)
                            .unwrap_or(false);
                        self.state = InteractionState::ConnectingPort {
                            source_port: port_id,
                            from_output,
                            cursor_world: world,
                        };
                    }
                    HitTarget::Node(node_id) => {
                        if modifiers.shift {
                            self.selection.toggle(node_id);
                        } else if !self.selection.is_selected(node_id) {
                            self.selection.clear();
                            self.selection.select(node_id);
                        }
                        effects.push(SideEffect::SelectionChanged);

                        // Begin dragging all selected nodes
                        let node_ids: Vec<EntityId> = self.selection.selected.clone();
                        let start_positions: Vec<(f64, f64)> = node_ids
                            .iter()
                            .map(|&id| {
                                graph.world.get::<NodePosition>(id)
                                    .map(|p| (p.x, p.y))
                                    .unwrap_or((0.0, 0.0))
                            })
                            .collect();
                        self.state = InteractionState::DraggingNodes {
                            node_ids,
                            start_positions,
                            last_world: world,
                        };
                    }
                    HitTarget::Frame(frame_id) => {
                        // Select the frame and all its member nodes
                        let members = cache.frame_rects.get(&frame_id)
                            .map(|(_, m)| m.clone())
                            .unwrap_or_default();
                        if !modifiers.shift {
                            self.selection.clear();
                        }
                        for &nid in &members {
                            self.selection.select(nid);
                        }
                        effects.push(SideEffect::FrameSelected(frame_id));
                        effects.push(SideEffect::SelectionChanged);

                        let node_ids: Vec<EntityId> = self.selection.selected.clone();
                        let start_positions: Vec<(f64, f64)> = node_ids
                            .iter()
                            .map(|&id| {
                                graph.world.get::<NodePosition>(id)
                                    .map(|p| (p.x, p.y))
                                    .unwrap_or((0.0, 0.0))
                            })
                            .collect();
                        self.state = InteractionState::DraggingNodes {
                            node_ids,
                            start_positions,
                            last_world: world,
                        };
                    }
                    HitTarget::Nothing => {
                        if !modifiers.shift {
                            self.selection.clear();
                            effects.push(SideEffect::SelectionChanged);
                        }
                        self.state = InteractionState::BoxSelecting {
                            start_world: world,
                            current_world: world,
                        };
                    }
                    _ => {}
                }
            }
            InputEvent::MouseDown { screen, button: MouseButton::Middle, .. } => {
                self.state = InteractionState::Panning { last_screen: screen };
            }
            InputEvent::MouseDown { world, button: MouseButton::Right, modifiers: Modifiers { ctrl: true, .. }, .. } => {
                self.state = InteractionState::CuttingLinks { points: vec![world] };
            }
            InputEvent::Scroll { screen, delta } => {
                let factor = if delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
                let new_zoom = self.viewport.zoom * factor;
                self.viewport.zoom_at(screen.x, screen.y, new_zoom);
            }
            _ => {}
        }
    }

    fn handle_panning(
        &mut self,
        event: InputEvent,
        last_screen: Vec2,
        _effects: &mut Vec<SideEffect>,
    ) {
        match event {
            InputEvent::MouseMove { screen, .. } => {
                let dx = screen.x - last_screen.x;
                let dy = screen.y - last_screen.y;
                self.viewport.pan_by(dx, dy);
                self.state = InteractionState::Panning { last_screen: screen };
            }
            InputEvent::MouseUp { button: MouseButton::Middle, .. } => {
                self.state = InteractionState::Idle;
            }
            _ => {}
        }
    }

    fn handle_dragging(
        &mut self,
        event: InputEvent,
        graph: &mut NodeGraph,
        node_ids: Vec<EntityId>,
        start_positions: Vec<(f64, f64)>,
        last_world: Vec2,
        effects: &mut Vec<SideEffect>,
    ) {
        match event {
            InputEvent::MouseMove { world, .. } => {
                let dx = world.x - last_world.x;
                let dy = world.y - last_world.y;
                for &nid in &node_ids {
                    if let Some(pos) = graph.world.get_mut::<NodePosition>(nid) {
                        pos.x += dx;
                        pos.y += dy;
                    }
                }
                effects.push(SideEffect::NodesMoved);
                self.state = InteractionState::DraggingNodes {
                    node_ids,
                    start_positions,
                    last_world: world,
                };
            }
            InputEvent::MouseUp { button: MouseButton::Left, .. } => {
                effects.push(SideEffect::NodesMoved);
                self.state = InteractionState::Idle;
            }
            _ => {}
        }
    }

    fn handle_connecting(
        &mut self,
        event: InputEvent,
        graph: &mut NodeGraph,
        cache: &LayoutCache,
        source_port: EntityId,
        from_output: bool,
        effects: &mut Vec<SideEffect>,
    ) {
        match event {
            InputEvent::MouseMove { world, .. } => {
                let source_pos = compute_port_world_position(graph, source_port)
                    .unwrap_or(world);
                let path = layout::compute_preview_path(source_pos, world, from_output);
                effects.push(SideEffect::PreviewWire { path });
                self.state = InteractionState::ConnectingPort {
                    source_port,
                    from_output,
                    cursor_world: world,
                };
            }
            InputEvent::MouseUp { world, button: MouseButton::Left, .. } => {
                let target = hit_test(graph, cache, world);
                if let HitTarget::Port(target_port) = target {
                    if target_port != source_port {
                        match graph.connect(source_port, target_port) {
                            Ok(conn_id) => effects.push(SideEffect::ConnectionCreated(conn_id)),
                            Err(_) => effects.push(SideEffect::ConnectionFailed),
                        }
                    }
                }
                effects.push(SideEffect::ClearTransient);
                self.state = InteractionState::Idle;
            }
            _ => {}
        }
    }

    fn handle_box_selecting(
        &mut self,
        event: InputEvent,
        _graph: &mut NodeGraph,
        cache: &LayoutCache,
        start_world: Vec2,
        effects: &mut Vec<SideEffect>,
    ) {
        match event {
            InputEvent::MouseMove { world, .. } => {
                let rect = Rect::from_corners(start_world, world);
                effects.push(SideEffect::BoxSelectRect { rect });
                self.state = InteractionState::BoxSelecting {
                    start_world,
                    current_world: world,
                };
            }
            InputEvent::MouseUp { world, modifiers, button: MouseButton::Left, .. } => {
                let rect = Rect::from_corners(start_world, world);
                let hits = hit_test_rect(cache, rect);
                if modifiers.shift {
                    for id in hits {
                        self.selection.select(id);
                    }
                } else {
                    self.selection.set(hits);
                }
                effects.push(SideEffect::ClearTransient);
                effects.push(SideEffect::SelectionChanged);
                self.state = InteractionState::Idle;
            }
            _ => {}
        }
    }

    fn handle_cutting(
        &mut self,
        event: InputEvent,
        graph: &mut NodeGraph,
        cache: &LayoutCache,
        mut points: Vec<Vec2>,
        effects: &mut Vec<SideEffect>,
    ) {
        match event {
            InputEvent::MouseMove { world, .. } => {
                points.push(world);
                self.state = InteractionState::CuttingLinks { points };
            }
            InputEvent::MouseUp { button: MouseButton::Right, .. } => {
                // Find all connections that intersect the cut line
                let mut to_disconnect = Vec::new();
                for (&conn_id, path) in &cache.connection_paths {
                    for window in points.windows(2) {
                        if bezier_intersects_segment(path, window[0], window[1]) {
                            to_disconnect.push(conn_id);
                            break;
                        }
                    }
                }
                for conn_id in to_disconnect {
                    graph.disconnect(conn_id);
                }
                effects.push(SideEffect::ClearTransient);
                self.state = InteractionState::Idle;
            }
            _ => {}
        }
    }
}

impl Default for InteractionController {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a bezier path approximately intersects a line segment.
/// Samples the bezier and checks if any adjacent sample pair crosses the segment.
fn bezier_intersects_segment(path: &BezierPath, seg_a: Vec2, seg_b: Vec2) -> bool {
    let samples = 20;
    for i in 0..samples {
        let t0 = i as f64 / samples as f64;
        let t1 = (i + 1) as f64 / samples as f64;
        let p0 = path.point_at(t0);
        let p1 = path.point_at(t1);
        if segments_intersect(p0, p1, seg_a, seg_b) {
            return true;
        }
    }
    false
}

/// Check if two line segments (a1-a2) and (b1-b2) intersect.
/// Uses non-strict inequality on one side so that a segment endpoint lying
/// exactly on the other segment counts as an intersection.
fn segments_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> bool {
    let d1 = cross(a1, a2, b1);
    let d2 = cross(a1, a2, b2);
    let d3 = cross(b1, b2, a1);
    let d4 = cross(b1, b2, a2);

    // Standard proper intersection
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }

    // Endpoint-on-segment cases (one cross product is ~0)
    if d1 * d2 <= 0.0 && d3 * d4 <= 0.0 {
        // At least one pair straddles — but reject the degenerate case where
        // both products are 0 and segments are collinear but non-overlapping.
        // For cut-link purposes, this is good enough.
        if (d1 != 0.0 || d2 != 0.0) && (d3 != 0.0 || d4 != 0.0) {
            return true;
        }
    }

    false
}

fn cross(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[cfg(test)]
mod tests;
