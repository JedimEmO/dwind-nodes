use std::rc::Rc;
use std::cell::Cell;

use wasm_bindgen::JsCast;
use dominator::{html, svg, Dom, clone, events, EventOptions};
use futures_signals::signal::SignalExt;
use futures_signals::signal_vec::SignalVecExt;
use futures_signals::map_ref;

use crate::graph_signals::{GraphSignals, ATTR_VIEWPORT_INNER};
use crate::node_view::render_node;
use crate::connection_view::{render_connection, render_preview_wire, render_cut_line};
use crate::frame_view::render_frame;
use crate::search_menu::render_search_menu;
use crate::event_bridge;

pub fn render_graph_editor(gs: Rc<GraphSignals>) -> Dom {
    let container_rect: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));

    // Outer HTML div for sizing and event capture
    html!("div", {
        .style("width", "100%")
        .style("height", "100%")
        .style("position", "relative")
        .style("overflow", "hidden")
        .style("background", "#1a1a2e")

        .after_inserted(clone!(container_rect => move |el| {
            let rect = el.get_bounding_client_rect();
            container_rect.set((rect.left(), rect.top()));
        }))

        .event_with_options(
            &EventOptions { preventable: true, ..EventOptions::default() },
            |e: events::ContextMenu| { e.prevent_default(); }
        )

        .event(clone!(gs, container_rect => move |e: events::MouseDown| {
            if gs.search_menu.get().is_some() {
                // Check if the click is inside the search menu
                if let Some(target) = e.target() {
                    let target_el: Result<web_sys::Element, _> = target.dyn_into();
                    if let Ok(el) = target_el {
                        if el.closest("[data-search-menu]").ok().flatten().is_some() {
                            return; // click inside menu — don't close
                        }
                    }
                }
                gs.close_search_menu();
                return;
            }
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

        // Keyboard shortcuts
        .attr("tabindex", "0")
        .style("outline", "none")
        .event_with_options(
            &EventOptions { preventable: true, ..EventOptions::default() },
            clone!(gs => move |e: events::KeyDown| {
                let key = e.key();
                let ctrl = e.ctrl_key();
                let shift = e.shift_key();
                // Escape always closes search menu first
                if key == "Escape" {
                    if gs.search_menu.get().is_some() {
                        gs.close_search_menu();
                        e.prevent_default();
                        return;
                    }
                }

                let handled = match key.as_str() {
                    "Delete" | "x" | "X" if !ctrl => { gs.delete_selected(); true }
                    "z" | "Z" if ctrl && shift => { gs.redo(); true }
                    "z" | "Z" if ctrl => { gs.undo(); true }
                    "d" | "D" if shift => { gs.duplicate_selected(); true }
                    "m" | "M" if !ctrl => { gs.toggle_mute_selected(); true }
                    "h" | "H" if !ctrl => { gs.toggle_collapse_selected(); true }
                    "a" | "A" if !ctrl && !shift => { gs.select_all(); true }
                    "f" | "F" if !ctrl && !shift => { gs.create_frame_around_selected(); true }
                    "g" | "G" if !ctrl && !shift => { gs.group_selected(); true }
                    "g" | "G" if shift && !ctrl => { gs.ungroup_selected(); true }
                    "+" | "=" => { gs.add_group_io_port(); true }
                    "a" | "A" if shift && !ctrl => {
                        // Open search menu at viewport center (world coords)
                        let (px, py) = gs.pan.get();
                        let z = gs.zoom.get();
                        let wx = (400.0 - px) / z; // approximate center
                        let wy = (300.0 - py) / z;
                        gs.open_search_menu(wx, wy);
                        true
                    }
                    _ => false,
                };
                if handled { e.prevent_default(); }
            })
        )

        // Single SVG element containing everything
        .child(svg!("svg", {
            .attr("width", "100%")
            .attr("height", "100%")
            .attr("xmlns", "http://www.w3.org/2000/svg")

            // Pan/zoom transform group
            .child(svg!("g", {
                .attr(ATTR_VIEWPORT_INNER, "")
                .attr_signal("transform", {
                    map_ref! {
                        let pan = gs.pan.signal(),
                        let zoom = gs.zoom.signal() => {
                            format!("translate({}, {}) scale({})", pan.0, pan.1, zoom)
                        }
                    }
                })

                // Frame layer (behind everything)
                .children_signal_vec(
                    gs.frame_list.signal_vec_cloned().map(clone!(gs => move |frame_id| {
                        render_frame(frame_id, &gs)
                    }))
                )

                // Connection layer (behind nodes)
                .children_signal_vec(
                    gs.connection_list.signal_vec_cloned().map(clone!(gs => move |conn_id| {
                        render_connection(conn_id, &gs)
                    }))
                )

                // Preview wire
                .child(render_preview_wire(&gs))

                // Cut line
                .child(render_cut_line(&gs))

                // Node layer (on top)
                .children_signal_vec(
                    gs.node_list.signal_vec_cloned().map(clone!(gs => move |node_id| {
                        render_node(node_id, &gs)
                    }))
                )

            }))

            // Box select overlay (in world space, inside the transform group would scale it;
            // keep in screen space by applying inverse transform or separate group)
        }))

        // Search menu (HTML, screen space, above SVG)
        .child(render_search_menu(&gs))

        // Breadcrumb navigation
        .child(html!("div", {
            .style("position", "absolute")
            .style("top", "8px")
            .style("left", "8px")
            .style("z-index", "50")
            .style("display", "flex")
            .style("gap", "4px")
            .style("align-items", "center")
            .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
            .style("font-size", "12px")

            .children_signal_vec(
                gs.breadcrumb.signal_vec_cloned().map(clone!(gs => move |(graph_id, label)| {
                    html!("span", {
                        .style("color", "#aaa")
                        .style("cursor", "pointer")
                        .style("padding", "4px 8px")
                        .style("background", "rgba(30,30,48,0.8)")
                        .style("border-radius", "4px")
                        .text(&format!("{} ›", label))
                        .event(clone!(gs => move |_: events::Click| {
                            gs.navigate_to_graph(graph_id);
                        }))
                    })
                }))
            )
        }))

        // Box select overlay in screen space (HTML div, outside SVG)
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
