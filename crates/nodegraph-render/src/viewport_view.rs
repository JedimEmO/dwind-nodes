use std::cell::Cell;
use std::rc::Rc;

use dominator::{clone, events, html, svg, Dom, EventOptions};
use futures_signals::map_ref;
use futures_signals::signal::SignalExt;
use futures_signals::signal_vec::SignalVecExt;
use wasm_bindgen::JsCast;

use crate::connection_view::{render_connection, render_cut_line, render_preview_wire};
use crate::context_menu::render_context_menu;
use crate::event_bridge;
use crate::frame_view::render_frame;
use crate::graph_signals::{GraphSignals, ATTR_VIEWPORT_INNER};
use crate::minimap_view::render_minimap;
use crate::node_view::render_node;
use crate::search_menu::render_search_menu;

/// Render a complete node graph editor as a DOM element.
///
/// The returned `Dom` fills its parent container (100% width/height) and provides:
/// pan (LMB-drag on empty canvas), zoom (scroll), node dragging, connection drawing,
/// box selection (Shift+LMB on empty canvas), cut links (Ctrl+RMB), search menu
/// (Shift+A), right-click context menu, minimap, and keyboard shortcuts.
///
/// ```rust,ignore
/// let gs = GraphSignals::new();
/// // ... register node types, add nodes, connect ...
/// dominator::append_dom(&dominator::body(), render_graph_editor(gs));
/// ```
pub fn render_graph_editor(gs: Rc<GraphSignals>) -> Dom {
    let container_rect: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));

    // Outer HTML div for sizing and event capture
    html!("div", {
        .style("width", "100%")
        .style("height", "100%")
        .style("position", "relative")
        .style("overflow", "hidden")
        .style("background", gs.theme.canvas_bg)
        .style_signal("cursor", gs.is_panning.signal().map(|p| if p { "grabbing" } else { "grab" }))

        .after_inserted(clone!(gs, container_rect => move |el| {
            let rect = el.get_bounding_client_rect();
            container_rect.set((rect.left(), rect.top()));
            gs.viewport_size.set((rect.width(), rect.height()));
        }))

        .event_with_options(
            &EventOptions { preventable: true, ..EventOptions::default() },
            clone!(gs, container_rect => move |e: events::ContextMenu| {
                e.prevent_default();
                // Ctrl+RMB is the cut-links gesture; suppress the context menu so it
                // doesn't flash/stick during or after the drag.
                if e.ctrl_key() {
                    return;
                }
                // Open context menu at click position with hit target
                let cr = container_rect.get();
                let screen_x = e.mouse_x() as f64 - cr.0;
                let screen_y = e.mouse_y() as f64 - cr.1;
                let (pan_x, pan_y) = gs.pan.get();
                let zoom = gs.zoom.get();
                let world_x = (screen_x - pan_x) / zoom;
                let world_y = (screen_y - pan_y) / zoom;

                let target = {
                    let editor = gs.editor.borrow();
                    let graph = editor.current_graph();
                    let cache = nodegraph_core::layout::LayoutCache::compute(graph);
                    nodegraph_core::interaction::hit_test(graph, &cache, nodegraph_core::layout::Vec2::new(world_x, world_y))
                };

                gs.context_menu.set(Some((target, world_x, world_y)));
            })
        )

        .event(clone!(gs, container_rect => move |e: events::MouseDown| {
            // Let port widgets (float scrub, color picker) handle their own events
            if let Some(target) = e.target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    if el.closest("[data-port-widget]").ok().flatten().is_some() {
                        return;
                    }
                }
            }
            // Close context menu on click outside
            if gs.context_menu.get().is_some() {
                if let Some(target) = e.target() {
                    let target_el: Result<web_sys::Element, _> = target.dyn_into();
                    if let Ok(el) = target_el {
                        if el.closest("[data-context-menu]").ok().flatten().is_some() {
                            return;
                        }
                    }
                }
                gs.context_menu.set(None);
                return;
            }
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
                // Escape closes overlays
                if key == "Escape" {
                    if gs.show_help.get() {
                        gs.show_help.set(false);
                        e.prevent_default();
                        return;
                    }
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
                    "a" | "A" if shift && !ctrl => {
                        let (wx, wy) = gs.cursor_world.get();
                        gs.open_search_menu(wx, wy);
                        true
                    }
                    "?" => { gs.show_help.set(!gs.show_help.get()); true }
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

        // Context menu (HTML, screen space)
        .child(render_context_menu(&gs))

        // Help button (bottom-left corner)
        .child(html!("div", {
            .style("position", "absolute")
            .style("bottom", "8px")
            .style("left", "8px")
            .style("z-index", "50")
            .style("width", "28px")
            .style("height", "28px")
            .style("border-radius", "14px")
            .style("background", "rgba(30, 30, 48, 0.8)")
            .style("border", &format!("1px solid {}", gs.theme.menu_border))
            .style("display", "flex")
            .style("align-items", "center")
            .style("justify-content", "center")
            .style("cursor", "pointer")
            .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
            .style("font-size", "14px")
            .style("font-weight", "bold")
            .style("color", "#888")
            .style("pointer-events", "auto")
            .text("?")
            .event(clone!(gs => move |_: events::Click| {
                gs.show_help.set(!gs.show_help.get());
            }))
        }))

        // Keyboard shortcut help overlay (? to toggle)
        .child(html!("div", {
            .style("position", "absolute")
            .style("top", "50%")
            .style("left", "50%")
            .style("transform", "translate(-50%, -50%)")
            .style("z-index", "200")
            .style("background", "rgba(20, 20, 35, 0.95)")
            .style("border", &format!("1px solid {}", gs.theme.menu_border))
            .style("border-radius", "8px")
            .style("padding", "20px 28px")
            .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
            .style("color", "#ccc")
            .style("font-size", "12px")
            .style("box-shadow", "0 8px 32px rgba(0,0,0,0.6)")
            .style("pointer-events", "auto")
            .style("max-width", "420px")
            .style_signal("display", gs.show_help.signal().map(|show| {
                if show { "block" } else { "none" }
            }))

            .child(html!("div", {
                .style("font-size", "14px")
                .style("font-weight", "bold")
                .style("color", "white")
                .style("margin-bottom", "12px")
                .text("Keyboard Shortcuts")
            }))

            .child(html!("div", {
                .style("display", "grid")
                .style("grid-template-columns", "auto 1fr")
                .style("gap", "4px 16px")
                .style("line-height", "1.6")

                .children(vec![
                    ("Shift+A", "Add node (search menu)"),
                    ("Delete / X", "Delete selected"),
                    ("Ctrl+Z", "Undo"),
                    ("Ctrl+Shift+Z", "Redo"),
                    ("Shift+D", "Duplicate"),
                    ("G", "Group selected"),
                    ("Shift+G", "Ungroup"),
                    ("F", "Create frame"),
                    ("H", "Collapse / Expand"),
                    ("M", "Mute / Unmute"),
                    ("A", "Select all / Deselect"),
                    ("Middle mouse", "Pan viewport"),
                    ("Scroll", "Zoom"),
                    ("Ctrl+RMB drag", "Cut links"),
                    ("Right-click", "Context menu"),
                    ("?", "Toggle this help"),
                ].into_iter().flat_map(|(key, desc)| {
                    vec![
                        html!("span", {
                            .style("color", gs.theme.selection_highlight)
                            .style("font-weight", "bold")
                            .style("font-family", "monospace")
                            .style("font-size", "11px")
                            .text(key)
                        }),
                        html!("span", { .text(desc) }),
                    ]
                }).collect::<Vec<_>>())
            }))

            .child(html!("div", {
                .style("margin-top", "12px")
                .style("color", "#666")
                .style("font-size", "10px")
                .style("text-align", "center")
                .text("Press ? or Escape to close")
            }))
        }))

        // Minimap (bottom-right corner)
        .child(render_minimap(&gs))

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
                        .style("color", gs.theme.breadcrumb_text)
                        .style("cursor", "pointer")
                        .style("padding", "4px 8px")
                        .style("background", gs.theme.breadcrumb_bg)
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
            .style("border", &format!("1px solid {}", gs.theme.box_select_border))
            .style("background", gs.theme.box_select_fill)
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
