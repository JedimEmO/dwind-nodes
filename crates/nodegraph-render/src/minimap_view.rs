use std::rc::Rc;
use std::cell::Cell;

use wasm_bindgen::JsCast;
use dominator::{html, svg, Dom, clone, events};
use futures_signals::signal::SignalExt;
use futures_signals::signal_vec::SignalVecExt;
use futures_signals::map_ref;

use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::graph::port::PortOwner;
use nodegraph_core::layout::{NODE_MIN_WIDTH, HEADER_HEIGHT, PORT_HEIGHT};
use nodegraph_core::store::EntityId;

use crate::graph_signals::GraphSignals;

const MINIMAP_WIDTH: f64 = 200.0;
const MINIMAP_HEIGHT: f64 = 150.0;
const MINIMAP_PADDING: f64 = 10.0;

/// Compute minimap transform: (offset_x, offset_y, scale) to fit graph bounds into minimap area.
fn minimap_transform(bounds: (f64, f64, f64, f64)) -> (f64, f64, f64) {
    let (min_x, min_y, max_x, max_y) = bounds;
    let graph_w = (max_x - min_x).max(1.0);
    let graph_h = (max_y - min_y).max(1.0);
    let usable_w = MINIMAP_WIDTH - 2.0 * MINIMAP_PADDING;
    let usable_h = MINIMAP_HEIGHT - 2.0 * MINIMAP_PADDING;
    let scale = (usable_w / graph_w).min(usable_h / graph_h);
    let offset_x = MINIMAP_PADDING + (usable_w - graph_w * scale) / 2.0;
    let offset_y = MINIMAP_PADDING + (usable_h - graph_h * scale) / 2.0;
    (offset_x, offset_y, scale)
}

pub fn render_minimap(gs: &Rc<GraphSignals>) -> Dom {
    let theme = &gs.theme;
    let dragging = Rc::new(Cell::new(false));
    let minimap_rect: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));

    html!("div", {
        .attr("data-minimap", "")
        .style("position", "absolute")
        .style("bottom", "8px")
        .style("right", "8px")
        .style("width", &format!("{}px", MINIMAP_WIDTH))
        .style("height", &format!("{}px", MINIMAP_HEIGHT))
        .style("background", theme.minimap_bg)
        .style("border", &format!("1px solid {}", theme.minimap_border))
        .style("border-radius", "4px")
        .style("overflow", "hidden")
        .style("z-index", "40")
        .style("cursor", "crosshair")

        .after_inserted(clone!(minimap_rect => move |el| {
            let rect = el.get_bounding_client_rect();
            minimap_rect.set((rect.left(), rect.top()));
        }))

        // Click/drag to pan
        .event(clone!(gs, dragging, minimap_rect => move |e: events::MouseDown| {
            e.stop_propagation();
            dragging.set(true);
            // Update minimap rect in case of layout shifts
            if let Some(target) = e.target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    let rect = el.closest("[data-minimap]").ok().flatten()
                        .map(|m| m.get_bounding_client_rect());
                    if let Some(r) = rect {
                        minimap_rect.set((r.left(), r.top()));
                    }
                }
            }
            pan_to_minimap_click(&gs, e.offset_x() as f64, e.offset_y() as f64);
        }))
        .global_event(clone!(gs, dragging, minimap_rect => move |e: events::MouseMove| {
            if dragging.get() {
                let (ml, mt) = minimap_rect.get();
                let mx = e.mouse_x() as f64 - ml;
                let my = e.mouse_y() as f64 - mt;
                pan_to_minimap_click(&gs, mx, my);
            }
        }))
        .global_event(clone!(dragging => move |_: events::MouseUp| {
            dragging.set(false);
        }))

        .child(svg!("svg", {
            .attr("width", "100%")
            .attr("height", "100%")

            // Node rectangles
            .children_signal_vec(
                gs.node_list.signal_vec_cloned().map(clone!(gs => move |node_id| {
                    render_minimap_node(node_id, &gs)
                }))
            )

            // Connection lines
            .children_signal_vec(
                gs.connection_list.signal_vec_cloned().map(clone!(gs => move |conn_id| {
                    render_minimap_connection(conn_id, &gs)
                }))
            )

            // Viewport rectangle
            .child(render_minimap_viewport(&gs))
        }))
    })
}

fn render_minimap_node(node_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let pos_signal = gs.get_node_position_signal(node_id);
    let header_signal = gs.get_node_header_signal(node_id);
    let num_ports = gs.with_graph(|g| g.node_ports(node_id).len());
    let node_h = HEADER_HEIGHT + num_ports as f64 * PORT_HEIGHT;

    let bounds = gs.graph_bounds.clone();

    match (pos_signal, header_signal) {
        (Some(pos), Some(header)) => {
            svg!("rect", {
                .attr_signal("x", map_ref! {
                    let (px, py) = pos.signal(),
                    let b = bounds.signal() => {
                        let (ox, _, s) = minimap_transform(*b);
                        format!("{}", ox + (px - b.0) * s)
                    }
                })
                .attr_signal("y", map_ref! {
                    let (px, py) = pos.signal(),
                    let b = bounds.signal() => {
                        let (_, oy, s) = minimap_transform(*b);
                        format!("{}", oy + (py - b.1) * s)
                    }
                })
                .attr_signal("width", bounds.signal().map(|b| {
                    let (_, _, s) = minimap_transform(b);
                    format!("{}", NODE_MIN_WIDTH * s)
                }))
                .attr_signal("height", bounds.signal().map(move |b| {
                    let (_, _, s) = minimap_transform(b);
                    format!("{}", node_h * s)
                }))
                .attr("rx", "1")
                .attr_signal("fill", header.signal_cloned().map(|h| {
                    format!("rgb({},{},{})", h.color[0], h.color[1], h.color[2])
                }))
                .attr("opacity", "0.8")
            })
        }
        _ => svg!("g", {}),
    }
}

fn render_minimap_connection(conn_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let editor = gs.editor.borrow();
    let graph = editor.current_graph();
    let ep = match graph.world.get::<ConnectionEndpoints>(conn_id) {
        Some(ep) => ep.clone(),
        None => return svg!("g", {}),
    };
    let src_owner = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0).unwrap_or(ep.source_port);
    let tgt_owner = graph.world.get::<PortOwner>(ep.target_port).map(|o| o.0).unwrap_or(ep.target_port);
    let conn_color = gs.theme.minimap_connection;
    drop(editor);

    let src_pos = gs.get_node_position_signal(src_owner);
    let tgt_pos = gs.get_node_position_signal(tgt_owner);
    let bounds = gs.graph_bounds.clone();

    match (src_pos, tgt_pos) {
        (Some(sp), Some(tp)) => {
            svg!("line", {
                .attr_signal("x1", map_ref! {
                    let (sx, _) = sp.signal(),
                    let b = bounds.signal() => {
                        let (ox, _, s) = minimap_transform(*b);
                        format!("{}", ox + (sx - b.0 + NODE_MIN_WIDTH / 2.0) * s)
                    }
                })
                .attr_signal("y1", map_ref! {
                    let (_, sy) = sp.signal(),
                    let b = bounds.signal() => {
                        let (_, oy, s) = minimap_transform(*b);
                        format!("{}", oy + (sy - b.1 + HEADER_HEIGHT / 2.0) * s)
                    }
                })
                .attr_signal("x2", map_ref! {
                    let (tx, _) = tp.signal(),
                    let b = bounds.signal() => {
                        let (ox, _, s) = minimap_transform(*b);
                        format!("{}", ox + (tx - b.0 + NODE_MIN_WIDTH / 2.0) * s)
                    }
                })
                .attr_signal("y2", map_ref! {
                    let (_, ty) = tp.signal(),
                    let b = bounds.signal() => {
                        let (_, oy, s) = minimap_transform(*b);
                        format!("{}", oy + (ty - b.1 + HEADER_HEIGHT / 2.0) * s)
                    }
                })
                .attr("stroke", conn_color)
                .attr("stroke-width", "1")
                .attr("opacity", "0.5")
            })
        }
        _ => svg!("g", {}),
    }
}

fn render_minimap_viewport(gs: &Rc<GraphSignals>) -> Dom {
    let theme = &gs.theme;
    let vp_fill = theme.minimap_viewport_fill;
    let vp_stroke = theme.minimap_viewport_stroke;

    svg!("rect", {
        .attr("data-minimap-viewport", "")
        .attr_signal("x", map_ref! {
            let pan = gs.pan.signal(),
            let zoom = gs.zoom.signal(),
            let bounds = gs.graph_bounds.signal(),
            let vp_size = gs.viewport_size.signal() => {
                let (ox, _, s) = minimap_transform(*bounds);
                let world_x = -pan.0 / zoom;
                format!("{}", ox + (world_x - bounds.0) * s)
            }
        })
        .attr_signal("y", map_ref! {
            let pan = gs.pan.signal(),
            let zoom = gs.zoom.signal(),
            let bounds = gs.graph_bounds.signal(),
            let vp_size = gs.viewport_size.signal() => {
                let (_, oy, s) = minimap_transform(*bounds);
                let world_y = -pan.1 / zoom;
                format!("{}", oy + (world_y - bounds.1) * s)
            }
        })
        .attr_signal("width", map_ref! {
            let zoom = gs.zoom.signal(),
            let bounds = gs.graph_bounds.signal(),
            let vp_size = gs.viewport_size.signal() => {
                let (_, _, s) = minimap_transform(*bounds);
                format!("{}", vp_size.0 / zoom * s)
            }
        })
        .attr_signal("height", map_ref! {
            let zoom = gs.zoom.signal(),
            let bounds = gs.graph_bounds.signal(),
            let vp_size = gs.viewport_size.signal() => {
                let (_, _, s) = minimap_transform(*bounds);
                format!("{}", vp_size.1 / zoom * s)
            }
        })
        .attr("fill", vp_fill)
        .attr("stroke", vp_stroke)
        .attr("stroke-width", "1")
    })
}

fn pan_to_minimap_click(gs: &Rc<GraphSignals>, mx: f64, my: f64) {
    let bounds = gs.graph_bounds.get();
    let (ox, oy, scale) = minimap_transform(bounds);
    let vp_size = gs.viewport_size.get();
    let zoom = gs.zoom.get();

    // Convert minimap pixel to world coordinates
    let world_x = bounds.0 + (mx - ox) / scale;
    let world_y = bounds.1 + (my - oy) / scale;

    // Center the viewport on this world position
    let pan_x = vp_size.0 / 2.0 - world_x * zoom;
    let pan_y = vp_size.1 / 2.0 - world_y * zoom;

    gs.pan.set((pan_x, pan_y));

    // Update controller viewport state
    gs.controller.borrow_mut().viewport.pan = (pan_x, pan_y);
}
