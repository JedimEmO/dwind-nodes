use std::rc::Rc;
use std::cell::Cell;

use dominator::{html, svg, Dom, clone, events, EventOptions};
use futures_signals::signal::SignalExt;
use futures_signals::signal_vec::SignalVecExt;
use futures_signals::map_ref;

use crate::graph_signals::GraphSignals;
use crate::node_view::render_node;
use crate::connection_view::{render_connection, render_preview_wire};
use crate::event_bridge;

pub fn render_graph_editor(gs: Rc<GraphSignals>) -> Dom {
    let container_rect: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));

    html!("div", {
        .style("width", "100%")
        .style("height", "100%")
        .style("position", "relative")
        .style("overflow", "hidden")
        .style("background", "#1a1a2e")

        // Track container position for coordinate conversion
        .after_inserted(clone!(container_rect => move |el| {
            let rect = el.get_bounding_client_rect();
            container_rect.set((rect.left(), rect.top()));
        }))

        // Prevent context menu
        .event_with_options(
            &EventOptions { preventable: true, ..EventOptions::default() },
            |e: events::ContextMenu| {
                e.prevent_default();
            }
        )

        // Mouse events
        .event(clone!(gs, container_rect => move |e: events::MouseDown| {
            event_bridge::on_mouse_down(&gs, e, container_rect.get());
        }))
        .global_event(clone!(gs, container_rect => move |e: events::MouseMove| {
            event_bridge::on_mouse_move(&gs, e, container_rect.get());
        }))
        .global_event(clone!(gs, container_rect => move |e: events::MouseUp| {
            event_bridge::on_mouse_up(&gs, e, container_rect.get());
        }))
        .event(clone!(gs, container_rect => move |e: events::Wheel| {
            event_bridge::on_wheel(&gs, e, container_rect.get());
        }))

        // Transform container (holds nodes + SVG)
        .child(html!("div", {
            .style("position", "absolute")
            .style("transform-origin", "0 0")
            .style("left", "0")
            .style("top", "0")
            .style("width", "0")
            .style("height", "0")
            .attr("data-viewport-inner", "")

            .style_signal("transform", {
                map_ref! {
                    let pan = gs.pan.signal(),
                    let zoom = gs.zoom.signal() => {
                        format!("translate({}px, {}px) scale({})", pan.0, pan.1, zoom)
                    }
                }
            })

            // SVG layer for connections (behind nodes via z-index)
            .child(html!("div", {
                .style("position", "absolute")
                .style("left", "0")
                .style("top", "0")
                .style("width", "0")
                .style("height", "0")
                .style("overflow", "visible")
                .style("pointer-events", "none")
                .style("z-index", "0")
                .child(svg!("svg", {
                    .attr("xmlns", "http://www.w3.org/2000/svg")
                    .attr("width", "1")
                    .attr("height", "1")
                    .attr("overflow", "visible")

                    // Connection paths
                    .children_signal_vec(
                        gs.connection_list.signal_vec_cloned().map(clone!(gs => move |conn_id| {
                            render_connection(conn_id, &gs)
                        }))
                    )

                    // Preview wire
                    .child(render_preview_wire(&gs))
                }))
            }))

            // Node layer (above SVG)
            .children_signal_vec(
                gs.node_list.signal_vec_cloned().map(clone!(gs => move |node_id| {
                    render_node(node_id, &gs)
                }))
            )
        }))

        // Box select overlay — world-space rect converted to screen-space
        .child(html!("div", {
            .style("position", "absolute")
            .style("pointer-events", "none")
            .style("border", "1px solid #4a9eff")
            .style("background", "rgba(74, 158, 255, 0.1)")
            .style_signal("display", gs.box_select_rect.signal_cloned().map(|r| {
                if r.is_some() { "block" } else { "none" }
            }))
            .style_signal("left", {
                map_ref! {
                    let rect = gs.box_select_rect.signal_cloned(),
                    let pan = gs.pan.signal(),
                    let zoom = gs.zoom.signal() => {
                        rect.map(|(x, _, _, _)| format!("{}px", x * zoom + pan.0))
                            .unwrap_or_else(|| "0px".to_string())
                    }
                }
            })
            .style_signal("top", {
                map_ref! {
                    let rect = gs.box_select_rect.signal_cloned(),
                    let pan = gs.pan.signal(),
                    let zoom = gs.zoom.signal() => {
                        rect.map(|(_, y, _, _)| format!("{}px", y * zoom + pan.1))
                            .unwrap_or_else(|| "0px".to_string())
                    }
                }
            })
            .style_signal("width", {
                map_ref! {
                    let rect = gs.box_select_rect.signal_cloned(),
                    let zoom = gs.zoom.signal() => {
                        rect.map(|(_, _, w, _)| format!("{}px", w * zoom))
                            .unwrap_or_else(|| "0px".to_string())
                    }
                }
            })
            .style_signal("height", {
                map_ref! {
                    let rect = gs.box_select_rect.signal_cloned(),
                    let zoom = gs.zoom.signal() => {
                        rect.map(|(_, _, _, h)| format!("{}px", h * zoom))
                            .unwrap_or_else(|| "0px".to_string())
                    }
                }
            })
        }))
    })
}
