use std::rc::Rc;

use dominator::{clone, events, html, Dom};
use dwind::prelude::*;
use futures_signals::map_ref;
use futures_signals::signal::SignalExt;
use wasm_bindgen::JsCast;

use crate::graph_signals::GraphSignals;

const FONT_STACK: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";

pub fn render_context_menu(gs: &Rc<GraphSignals>) -> Dom {
    html!("div", {
        .dwclass!("absolute min-w-40 overflow-hidden rounded-md border border-gray-600 bg-bunker-800 text-xs text-gray-300 pointer-events-auto")
        // z-index 110 is above dwind's scale; keep raw.
        .style("z-index", "110")
        .style("font-family", FONT_STACK)
        .style("box-shadow", gs.theme.menu_shadow)

        .attr("data-context-menu", "")

        .style_signal("display", gs.context_menu.signal_cloned().map(|opt| {
            if opt.is_some() { "block" } else { "none" }
        }))

        .style_signal("left", {
            map_ref! {
                let menu = gs.context_menu.signal_cloned(),
                let pan = gs.pan.signal(),
                let zoom = gs.zoom.signal() => {
                    menu.as_ref().map(|(_, x, _)| format!("{}px", x * zoom + pan.0))
                        .unwrap_or_else(|| "0px".to_string())
                }
            }
        })
        .style_signal("top", {
            map_ref! {
                let menu = gs.context_menu.signal_cloned(),
                let pan = gs.pan.signal(),
                let zoom = gs.zoom.signal() => {
                    menu.as_ref().map(|(_, _, y)| format!("{}px", y * zoom + pan.1))
                        .unwrap_or_else(|| "0px".to_string())
                }
            }
        })

        // Menu items — reactive based on target type
        .child_signal(gs.context_menu.signal_cloned().map(clone!(gs => move |menu_opt| {
            let (target, _, _) = menu_opt?;
            use nodegraph_core::interaction::HitTarget;

            type MenuItem<'a> = (&'a str, Rc<dyn Fn()>);
            let mut items: Vec<MenuItem> = Vec::new();

            match target {
                HitTarget::Node(node_id) => {
                    let gs2 = gs.clone();
                    items.push(("Delete", Rc::new(move || {
                        gs2.select_single(node_id);
                        gs2.delete_selected();
                        gs2.context_menu.set(None);
                    })));
                    let gs2 = gs.clone();
                    items.push(("Duplicate", Rc::new(move || {
                        gs2.select_single(node_id);
                        gs2.duplicate_selected();
                        gs2.context_menu.set(None);
                    })));
                    let gs2 = gs.clone();
                    items.push(("Mute/Unmute", Rc::new(move || {
                        gs2.select_single(node_id);
                        gs2.toggle_mute_selected();
                        gs2.context_menu.set(None);
                    })));
                    let gs2 = gs.clone();
                    items.push(("Collapse/Expand", Rc::new(move || {
                        gs2.select_single(node_id);
                        gs2.toggle_collapse_selected();
                        gs2.context_menu.set(None);
                    })));
                }
                HitTarget::Connection(conn_id) => {
                    let gs2 = gs.clone();
                    items.push(("Delete Connection", Rc::new(move || {
                        gs2.save_undo();
                        gs2.with_graph_mut(|g| g.disconnect(conn_id));
                        gs2.reconcile_connections_pub();
                        gs2.context_menu.set(None);
                    })));
                }
                HitTarget::Frame(frame_id) => {
                    let gs2 = gs.clone();
                    items.push(("Delete Frame", Rc::new(move || {
                        gs2.save_undo();
                        gs2.with_graph_mut(|g| g.remove_frame(frame_id));
                        gs2.selected_frames.set(Vec::new());
                        gs2.full_sync_pub();
                        gs2.context_menu.set(None);
                    })));
                    // Color presets
                    let colors: &[(&str, [u8; 3])] = &[
                        ("Red", [200, 80, 80]),
                        ("Orange", [200, 140, 60]),
                        ("Yellow", [200, 200, 60]),
                        ("Green", [80, 180, 80]),
                        ("Cyan", [60, 180, 200]),
                        ("Blue", [80, 100, 200]),
                        ("Purple", [160, 80, 200]),
                        ("Gray", [120, 120, 140]),
                    ];
                    for &(name, color) in colors {
                        let gs2 = gs.clone();
                        items.push((name, Rc::new(move || {
                            gs2.save_undo();
                            gs2.with_graph_mut(|g| {
                                g.world.insert(frame_id, nodegraph_core::graph::frame::FrameColor(color));
                            });
                            gs2.full_sync_pub();
                            gs2.context_menu.set(None);
                        })));
                    }
                }
                HitTarget::Nothing => {
                    let gs2 = gs.clone();
                    items.push(("Add Node (Shift+A)", Rc::new(move || {
                        let pos = gs2.context_menu.get().map(|(_, x, y)| (x, y)).unwrap_or((0.0, 0.0));
                        gs2.context_menu.set(None);
                        gs2.open_search_menu(pos.0, pos.1);
                    })));
                }
                _ => {}
            }

            Some(html!("div", {
                .children(items.into_iter().map(|(label, action)| {
                    html!("div", {
                        .dwclass!("py-1.5 px-3 cursor-pointer")
                        .style("transition", "background 0.1s")
                        .event(clone!(action => move |_: events::Click| { (action)(); }))
                        .event(clone!(gs => move |e: events::MouseEnter| {
                            if let Some(t) = e.target() {
                                if let Ok(el) = t.dyn_into::<web_sys::HtmlElement>() {
                                    el.style().set_property("background", gs.theme.menu_selected_bg).ok();
                                }
                            }
                        }))
                        .event(|e: events::MouseLeave| {
                            if let Some(t) = e.target() {
                                if let Ok(el) = t.dyn_into::<web_sys::HtmlElement>() {
                                    el.style().set_property("background", "transparent").ok();
                                }
                            }
                        })
                        .text(label)
                    })
                }).collect::<Vec<_>>())
            }))
        })))
    })
}
