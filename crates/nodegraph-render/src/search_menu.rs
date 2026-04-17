use std::rc::Rc;

use dominator::{clone, events, html, Dom, EventOptions};
use dwind::prelude::*;
use futures_signals::map_ref;
use futures_signals::signal::{Mutable, SignalExt};
use wasm_bindgen::JsCast;

use crate::graph_signals::GraphSignals;

const FONT_STACK: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";

/// Search menu as an absolutely-positioned HTML div, outside the SVG.
pub fn render_search_menu(gs: &Rc<GraphSignals>) -> Dom {
    let search_text = Mutable::new(String::new());
    let selected_index = Mutable::new(0_usize);

    html!("div", {
        .dwclass!("absolute w-56 overflow-hidden rounded-md border border-gray-600 bg-bunker-800 text-xs text-gray-300")
        // z-index 100 and max-height 300px are off dwind's scales; keep raw.
        .style("z-index", "100")
        .style("max-height", "300px")
        .style("font-family", FONT_STACK)
        .style("box-shadow", gs.theme.menu_shadow)

        .style_signal("display", gs.search_menu.signal_cloned().map(|opt| {
            if opt.is_some() { "block" } else { "none" }
        }))

        .style_signal("left", {
            map_ref! {
                let menu = gs.search_menu.signal_cloned(),
                let pan = gs.pan.signal(),
                let zoom = gs.zoom.signal() => {
                    menu.map(|(x, _)| format!("{}px", x * zoom + pan.0))
                        .unwrap_or_else(|| "0px".to_string())
                }
            }
        })
        .style_signal("top", {
            map_ref! {
                let menu = gs.search_menu.signal_cloned(),
                let pan = gs.pan.signal(),
                let zoom = gs.zoom.signal() => {
                    menu.map(|(_, y)| format!("{}px", y * zoom + pan.1))
                        .unwrap_or_else(|| "0px".to_string())
                }
            }
        })

        .attr("data-search-menu", "")

        // Search input
        .child(html!("input", {
            .attr("type", "text")
            .attr("placeholder", "Search nodes...")
            .dwclass!("w-full py-2 px-2.5 bg-bunker-900 text-white-50 text-xs")
            // border-none + bottom-only-border: express as raw, since dwind has no border-b-1 shorthand with color in one class
            .style("border", "none")
            .style("border-bottom", &format!("1px solid {}", gs.theme.menu_input_border))
            .style("outline", "none")

            .event(clone!(search_text, selected_index => move |e: events::Input| {
                if let Some(target) = e.target() {
                    if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                        search_text.set(input.value());
                        selected_index.set(0);
                    }
                }
            }))

            .event_with_options(
                &EventOptions { preventable: true, ..EventOptions::default() },
                clone!(gs, search_text, selected_index => move |e: events::KeyDown| {
                    e.stop_propagation();
                    match e.key().as_str() {
                        "Escape" => {
                            e.prevent_default();
                            gs.close_search_menu();
                        }
                        "ArrowDown" => {
                            e.prevent_default();
                            selected_index.set(selected_index.get() + 1);
                        }
                        "ArrowUp" => {
                            e.prevent_default();
                            let idx = selected_index.get();
                            if idx > 0 { selected_index.set(idx - 1); }
                        }
                        "Enter" => {
                            e.prevent_default();
                            let query = search_text.get_cloned();
                            let idx = selected_index.get();
                            let reg = gs.registry.borrow();
                            let pending = gs.pending_connection.get();
                            let results: Vec<_> = if let Some((_, src_type, from_output)) = pending {
                                reg.search_compatible(&query, src_type, from_output)
                            } else {
                                reg.search(&query)
                            };
                            if let Some(def) = results.get(idx) {
                                let type_id = def.type_id.clone();
                                let pos = gs.search_menu.get().unwrap_or((0.0, 0.0));
                                drop(reg);
                                gs.spawn_from_registry(&type_id, pos);
                            }
                        }
                        _ => {}
                    }
                })
            )

            // Focus input when menu opens (signal-driven, not just on DOM creation)
            .after_inserted(clone!(gs => move |el| {
                // Focus immediately if menu is already open
                if gs.search_menu.get().is_some() {
                    let _ = el.focus();
                }
            }))
        }))

        // Results list
        .child(html!("div", {
            .dwclass!("overflow-y-auto")
            // max-height 260px isn't on the scale; keep raw.
            .style("max-height", "260px")

            .child_signal(map_ref! {
                let query = search_text.signal_cloned(),
                let pending = gs.pending_connection.signal() => {
                    (query.clone(), *pending)
                }
            }.map(clone!(gs, selected_index => move |(query, pending)| {
                let reg = gs.registry.borrow();
                let results: Vec<_> = if let Some((_, src_type, from_output)) = pending {
                    reg.search_compatible(&query, src_type, from_output)
                } else {
                    reg.search(&query)
                };
                let idx = selected_index.get();

                Some(html!("div", {
                    .children(results.iter().enumerate().map(|(i, def)| {
                        let type_id = def.type_id.clone();
                        let name = def.display_name.clone();
                        let category = def.category.clone();
                        let is_selected = i == idx;

                        html!("div", {
                            .dwclass!("py-1.5 px-2.5 cursor-pointer")
                            .style("background", if is_selected { gs.theme.menu_selected_bg } else { "transparent" })
                            .style("border-left", &{
                                if is_selected {
                                    format!("3px solid {}", gs.theme.menu_selected_border)
                                } else {
                                    "3px solid transparent".to_string()
                                }
                            })

                            .child(html!("div", {
                                .dwclass!("font-bold text-white-50")
                                .text(&name)
                            }))
                            .child(html!("div", {
                                .dwclass!("text-gray-500")
                                // 9px is below dwind's rem scale; keep raw.
                                .style("font-size", "9px")
                                .text(&category)
                            }))

                            .event(clone!(gs, type_id => move |_: events::Click| {
                                let pos = gs.search_menu.get().unwrap_or((0.0, 0.0));
                                gs.spawn_from_registry(&type_id, pos);
                            }))
                        })
                    }).collect::<Vec<_>>())
                }))
            })))
        }))
    })
}
