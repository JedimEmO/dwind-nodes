use std::rc::Rc;

use dominator::{Dom, svg};
use futures_signals::signal::{Mutable, SignalExt};
use futures_signals::map_ref;

use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::graph::port::{PortOwner, PortSocketType, PortIndex, PortDirection};
use nodegraph_core::layout::{self, Vec2, HEADER_HEIGHT, PORT_HEIGHT, NODE_MIN_WIDTH};
use nodegraph_core::store::EntityId;

use crate::graph_signals::GraphSignals;

/// Compute port offset within its node — same formula as SVG circle placement in node_view.
fn port_offset(graph: &nodegraph_core::graph::NodeGraph, port_id: EntityId) -> Option<Vec2> {
    let owner = graph.world.get::<PortOwner>(port_id)?.0;
    let dir = *graph.world.get::<PortDirection>(port_id)?;

    // Reroute nodes place ports at the diamond edges, not standard layout positions
    let is_reroute = graph.world.get::<nodegraph_core::graph::reroute::IsReroute>(owner).is_some();
    if is_reroute {
        let size = layout::REROUTE_SIZE;
        let cx = match dir {
            PortDirection::Input => -size,
            PortDirection::Output => size,
        };
        return Some(Vec2::new(cx, 0.0));
    }

    let idx = graph.world.get::<PortIndex>(port_id)?.0 as f64;
    let cx = match dir {
        PortDirection::Input => 0.0,
        PortDirection::Output => NODE_MIN_WIDTH,
    };
    let cy = HEADER_HEIGHT + (idx + 0.5) * PORT_HEIGHT;
    Some(Vec2::new(cx, cy))
}

pub fn render_connection(conn_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let editor = gs.editor.borrow();
    let graph = editor.current_graph();
    let ep = match graph.world.get::<ConnectionEndpoints>(conn_id) {
        Some(ep) => ep.clone(),
        None => return svg!("g", {}),
    };

    let src_owner = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0).unwrap_or(ep.source_port);
    let tgt_owner = graph.world.get::<PortOwner>(ep.target_port).map(|o| o.0).unwrap_or(ep.target_port);
    let src_offset = port_offset(&graph, ep.source_port).unwrap_or(Vec2::new(0.0, 0.0));
    let tgt_offset = port_offset(&graph, ep.target_port).unwrap_or(Vec2::new(0.0, 0.0));

    let src_type = graph.world.get::<PortSocketType>(ep.source_port).map(|s| s.0);
    let tgt_type = graph.world.get::<PortSocketType>(ep.target_port).map(|s| s.0);
    let src_color = src_type.map(|t| t.default_color()).unwrap_or([170, 170, 170]);
    let tgt_color = tgt_type.map(|t| t.default_color()).unwrap_or([170, 170, 170]);
    let is_conversion = match (src_type, tgt_type) { (Some(s), Some(t)) => s != t, _ => false };
    drop(editor);

    let src_pos = gs.get_node_position_signal(src_owner).unwrap_or_else(|| Mutable::new((0.0, 0.0)));
    let tgt_pos = gs.get_node_position_signal(tgt_owner).unwrap_or_else(|| Mutable::new((0.0, 0.0)));

    // Reactive bezier — recomputes when either node moves.
    // Uses SAME offset formula as SVG circle placement.
    let d_signal = map_ref! {
        let (sx, sy) = src_pos.signal(),
        let (tx, ty) = tgt_pos.signal() => move {
            let src = Vec2::new(sx + src_offset.x, sy + src_offset.y);
            let tgt = Vec2::new(tx + tgt_offset.x, ty + tgt_offset.y);
            layout::compute_connection_path(src, tgt).to_svg_d()
        }
    };

    if !is_conversion {
        let color = format!("rgb({},{},{})", src_color[0], src_color[1], src_color[2]);
        svg!("path", {
            .attr_signal("d", d_signal)
            .attr("fill", "none")
            .attr("stroke", &color)
            .attr("stroke-width", "2")
        })
    } else {
        // Gradient from source to target color
        let grad_id = format!("conn-grad-{}", conn_id.index);
        let src_css = format!("rgb({},{},{})", src_color[0], src_color[1], src_color[2]);
        let tgt_css = format!("rgb({},{},{})", tgt_color[0], tgt_color[1], tgt_color[2]);
        let stroke_url = format!("url(#{})", grad_id);

        let grad_src = gs.get_node_position_signal(src_owner).unwrap_or_else(|| Mutable::new((0.0, 0.0)));
        let grad_tgt = gs.get_node_position_signal(tgt_owner).unwrap_or_else(|| Mutable::new((0.0, 0.0)));

        svg!("g", {
            .child(svg!("defs", {
                .child(svg!("linearGradient", {
                    .attr("id", &grad_id)
                    .attr("gradientUnits", "userSpaceOnUse")
                    .attr_signal("x1", grad_src.signal().map(move |(x, _)| format!("{}", x + src_offset.x)))
                    .attr_signal("y1", grad_src.signal().map(move |(_, y)| format!("{}", y + src_offset.y)))
                    .attr_signal("x2", grad_tgt.signal().map(move |(x, _)| format!("{}", x + tgt_offset.x)))
                    .attr_signal("y2", grad_tgt.signal().map(move |(_, y)| format!("{}", y + tgt_offset.y)))
                    .child(svg!("stop", { .attr("offset", "0%").attr("stop-color", &src_css) }))
                    .child(svg!("stop", { .attr("offset", "100%").attr("stop-color", &tgt_css) }))
                }))
            }))
            .child(svg!("path", {
                .attr_signal("d", d_signal)
                .attr("fill", "none")
                .attr("stroke", &stroke_url)
                .attr("stroke-width", "2")
            }))
        })
    }
}

pub fn render_cut_line(gs: &Rc<GraphSignals>) -> Dom {
    let points = gs.cut_line_points.clone();
    svg!("polyline", {
        .attr("fill", "none")
        .attr("stroke", gs.theme.cut_line)
        .attr("stroke-width", "2")
        .attr("stroke-dasharray", "4,3")
        .attr_signal("points", points.signal_cloned().map(|pts| {
            pts.iter().map(|(x, y)| format!("{},{}", x, y)).collect::<Vec<_>>().join(" ")
        }))
        .attr_signal("visibility", points.signal_cloned().map(|pts| {
            if pts.is_empty() { "hidden" } else { "visible" }
        }))
    })
}

pub fn render_preview_wire(gs: &Rc<GraphSignals>) -> Dom {
    let preview = gs.preview_wire.clone();
    svg!("path", {
        .attr_signal("d", preview.signal_cloned().map(|opt| {
            opt.map(|p| p.to_svg_d()).unwrap_or_default()
        }))
        .attr("fill", "none")
        .attr("stroke", gs.theme.preview_wire)
        .attr("stroke-width", "2")
        .attr("stroke-dasharray", "6,4")
        .attr_signal("visibility", preview.signal_cloned().map(|opt| {
            if opt.is_some() { "visible" } else { "hidden" }
        }))
    })
}
