use crate::graph::NodeGraph;
use crate::graph::node::{NodeHeader, NodePosition};
use crate::graph::port::{PortDirection, PortIndex, PortOwner};
use crate::store::EntityId;

// Layout constants — matches typical Blender node proportions
pub const HEADER_HEIGHT: f64 = 28.0;
pub const PORT_HEIGHT: f64 = 22.0;
pub const PORT_RADIUS: f64 = 6.0;
pub const NODE_MIN_WIDTH: f64 = 160.0;
pub const REROUTE_SIZE: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: Vec2) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }

    /// Create a normalized rect from two arbitrary corner points.
    pub fn from_corners(a: Vec2, b: Vec2) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let w = (a.x - b.x).abs();
        let h = (a.y - b.y).abs();
        Self { x, y, w, h }
    }
}

#[derive(Clone, Debug)]
pub struct ComputedNodeLayout {
    pub header_rect: Rect,
    pub body_rect: Rect,
    pub total_rect: Rect,
    pub input_port_positions: Vec<(EntityId, Vec2)>,
    pub output_port_positions: Vec<(EntityId, Vec2)>,
}

pub fn compute_node_layout(graph: &NodeGraph, node_id: EntityId) -> Option<ComputedNodeLayout> {
    let pos = graph.world.get::<NodePosition>(node_id)?;
    let header = graph.world.get::<NodeHeader>(node_id)?;

    let ports = graph.node_ports(node_id);
    let mut inputs: Vec<(EntityId, u32)> = Vec::new();
    let mut outputs: Vec<(EntityId, u32)> = Vec::new();

    for &port_id in ports {
        let dir = graph.world.get::<PortDirection>(port_id)?;
        let idx = graph.world.get::<PortIndex>(port_id).map(|i| i.0).unwrap_or(0);
        match dir {
            PortDirection::Input => inputs.push((port_id, idx)),
            PortDirection::Output => outputs.push((port_id, idx)),
        }
    }

    inputs.sort_by_key(|&(_, idx)| idx);
    outputs.sort_by_key(|&(_, idx)| idx);

    let num_rows = inputs.len().max(outputs.len());
    let body_height = if header.collapsed {
        0.0
    } else {
        num_rows as f64 * PORT_HEIGHT
    };

    let node_width = NODE_MIN_WIDTH;
    let total_height = HEADER_HEIGHT + body_height;

    let header_rect = Rect::new(pos.x, pos.y, node_width, HEADER_HEIGHT);
    let body_rect = Rect::new(pos.x, pos.y + HEADER_HEIGHT, node_width, body_height);
    let total_rect = Rect::new(pos.x, pos.y, node_width, total_height);

    let input_port_positions = if header.collapsed {
        Vec::new()
    } else {
        inputs
            .iter()
            .enumerate()
            .map(|(i, &(port_id, _))| {
                let py = pos.y + HEADER_HEIGHT + (i as f64 + 0.5) * PORT_HEIGHT;
                (port_id, Vec2::new(pos.x, py))
            })
            .collect()
    };

    let output_port_positions = if header.collapsed {
        Vec::new()
    } else {
        outputs
            .iter()
            .enumerate()
            .map(|(i, &(port_id, _))| {
                let py = pos.y + HEADER_HEIGHT + (i as f64 + 0.5) * PORT_HEIGHT;
                (port_id, Vec2::new(pos.x + node_width, py))
            })
            .collect()
    };

    Some(ComputedNodeLayout {
        header_rect,
        body_rect,
        total_rect,
        input_port_positions,
        output_port_positions,
    })
}

/// Compute the world position of a specific port by finding its node's layout.
pub fn compute_port_world_position(graph: &NodeGraph, port_id: EntityId) -> Option<Vec2> {
    let owner = graph.world.get::<PortOwner>(port_id)?;
    let layout = compute_node_layout(graph, owner.0)?;

    for &(pid, pos) in &layout.input_port_positions {
        if pid == port_id {
            return Some(pos);
        }
    }
    for &(pid, pos) in &layout.output_port_positions {
        if pid == port_id {
            return Some(pos);
        }
    }
    None
}

// ============================================================
// Bezier connection paths
// ============================================================

#[derive(Clone, Debug, PartialEq)]
pub struct BezierPath {
    pub start: Vec2,
    pub cp1: Vec2,
    pub cp2: Vec2,
    pub end: Vec2,
}

impl BezierPath {
    pub fn to_svg_d(&self) -> String {
        format!(
            "M {} {} C {} {}, {} {}, {} {}",
            self.start.x, self.start.y,
            self.cp1.x, self.cp1.y,
            self.cp2.x, self.cp2.y,
            self.end.x, self.end.y,
        )
    }

    /// Sample a point on the bezier at parameter t (0..1).
    pub fn point_at(&self, t: f64) -> Vec2 {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        Vec2 {
            x: mt3 * self.start.x
                + 3.0 * mt2 * t * self.cp1.x
                + 3.0 * mt * t2 * self.cp2.x
                + t3 * self.end.x,
            y: mt3 * self.start.y
                + 3.0 * mt2 * t * self.cp1.y
                + 3.0 * mt * t2 * self.cp2.y
                + t3 * self.end.y,
        }
    }

    /// Approximate minimum distance from a point to this bezier curve.
    pub fn distance_to_point(&self, point: Vec2) -> f64 {
        let samples = 20;
        let mut min_dist = f64::MAX;
        for i in 0..=samples {
            let t = i as f64 / samples as f64;
            let p = self.point_at(t);
            let dist = point.distance_to(p);
            if dist < min_dist {
                min_dist = dist;
            }
        }
        min_dist
    }
}

/// Compute a Blender-style bezier connection from source (output) to target (input).
/// Control points extend horizontally: cp1 goes right from source, cp2 goes left from target.
/// The offset is proportional to the horizontal distance, with a minimum.
pub fn compute_connection_path(source_pos: Vec2, target_pos: Vec2) -> BezierPath {
    let dx = (target_pos.x - source_pos.x).abs();
    let offset = (dx * 0.5).max(50.0);

    BezierPath {
        start: source_pos,
        cp1: Vec2::new(source_pos.x + offset, source_pos.y),
        cp2: Vec2::new(target_pos.x - offset, target_pos.y),
        end: target_pos,
    }
}

/// Compute a preview wire path during connection drag.
/// `from_output` indicates whether the drag started from an output port.
pub fn compute_preview_path(source_pos: Vec2, cursor_pos: Vec2, from_output: bool) -> BezierPath {
    if from_output {
        compute_connection_path(source_pos, cursor_pos)
    } else {
        compute_connection_path(cursor_pos, source_pos)
    }
}

pub const FRAME_PADDING: f64 = 30.0;

/// Compute the bounding rect of a frame from its member node positions.
pub fn compute_frame_rect(graph: &NodeGraph, member_ids: &[EntityId]) -> Rect {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for &nid in member_ids {
        if let Some(pos) = graph.world.get::<NodePosition>(nid) {
            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
            let num_ports = graph.node_ports(nid).len();
            let h = HEADER_HEIGHT + num_ports as f64 * PORT_HEIGHT;
            max_x = max_x.max(pos.x + NODE_MIN_WIDTH);
            max_y = max_y.max(pos.y + h);
        }
    }

    if min_x == f64::MAX {
        return Rect::new(0.0, 0.0, 200.0, 100.0);
    }

    Rect::new(
        min_x - FRAME_PADDING,
        min_y - FRAME_PADDING,
        (max_x - min_x) + FRAME_PADDING * 2.0,
        (max_y - min_y) + FRAME_PADDING * 2.0,
    )
}

/// Precomputed layout cache for all nodes in a graph.
/// Used by hit testing and rendering to avoid recomputing layouts repeatedly.
pub struct LayoutCache {
    pub layouts: std::collections::HashMap<EntityId, ComputedNodeLayout>,
    pub connection_paths: std::collections::HashMap<EntityId, BezierPath>,
    pub frame_rects: std::collections::HashMap<EntityId, (Rect, Vec<EntityId>)>,
}

impl LayoutCache {
    pub fn compute(graph: &NodeGraph) -> Self {
        use crate::graph::connection::ConnectionEndpoints;
        use crate::graph::frame::{FrameRect, FrameMembers};

        let mut layouts = std::collections::HashMap::new();
        for (node_id, _) in graph.world.query::<NodeHeader>() {
            if let Some(layout) = compute_node_layout(graph, node_id) {
                layouts.insert(node_id, layout);
            }
        }

        let mut connection_paths = std::collections::HashMap::new();
        for (conn_id, endpoints) in graph.world.query::<ConnectionEndpoints>() {
            let src_pos = compute_port_world_position(graph, endpoints.source_port);
            let tgt_pos = compute_port_world_position(graph, endpoints.target_port);
            if let (Some(src), Some(tgt)) = (src_pos, tgt_pos) {
                connection_paths.insert(conn_id, compute_connection_path(src, tgt));
            }
        }

        let mut frame_rects = std::collections::HashMap::new();
        for (frame_id, _) in graph.world.query::<FrameRect>() {
            if let Some(members) = graph.world.get::<FrameMembers>(frame_id) {
                let rect = compute_frame_rect(graph, &members.0);
                frame_rects.insert(frame_id, (rect, members.0.clone()));
            }
        }

        Self { layouts, connection_paths, frame_rects }
    }

    pub fn node_layout(&self, node_id: EntityId) -> Option<&ComputedNodeLayout> {
        self.layouts.get(&node_id)
    }
}
