use std::rc::Rc;

use dominator::{html, Dom, clone, svg, events, EventOptions};
use futures_signals::signal::{Mutable, SignalExt};
use futures_signals::map_ref;

use nodegraph_core::graph::node::{NodeHeader, MuteState};
use nodegraph_core::graph::port::{PortDirection, PortSocketType, PortLabel, PortIndex};
use nodegraph_core::layout::{HEADER_HEIGHT, PORT_HEIGHT, NODE_MIN_WIDTH, PORT_RADIUS, Vec2};
use nodegraph_core::store::EntityId;
use nodegraph_core::types::socket_type::SocketType;

use crate::graph_signals::{GraphSignals, ATTR_NODE_ID, ATTR_PORT_ID, is_valid_connection_target};

pub fn render_node(node_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let pos_signal = gs.get_node_position_signal(node_id)
        .unwrap_or_else(|| Mutable::new((0.0, 0.0)));
    let header_signal = gs.get_node_header_signal(node_id)
        .unwrap_or_else(|| Mutable::new(NodeHeader {
            title: "?".to_string(), color: [100, 100, 100], collapsed: false,
        }));
    let selection = gs.selection.clone();

    let editor = gs.editor.borrow();
    let graph = editor.current_graph();
    let ports = graph.node_ports(node_id).to_vec();
    let mut input_ports = Vec::new();
    let mut output_ports = Vec::new();
    for &pid in &ports {
        let dir = graph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Input);
        let st = graph.world.get::<PortSocketType>(pid).map(|s| s.0).unwrap_or(SocketType::Float);
        let label = graph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
        let idx = graph.world.get::<PortIndex>(pid).map(|i| i.0).unwrap_or(0);
        match dir {
            PortDirection::Input => input_ports.push((pid, st, label, idx)),
            PortDirection::Output => output_ports.push((pid, st, label, idx)),
        }
    }
    input_ports.sort_by_key(|&(_, _, _, idx)| idx);
    output_ports.sort_by_key(|&(_, _, _, idx)| idx);

    let header = graph.world.get::<NodeHeader>(node_id).cloned()
        .unwrap_or(NodeHeader { title: "?".to_string(), color: [100, 100, 100], collapsed: false });
    let is_muted = graph.world.get::<MuteState>(node_id).map(|m| m.0).unwrap_or(false);
    let is_group = graph.world.get::<nodegraph_core::graph::group::SubgraphRoot>(node_id).is_some();
    let is_group_io = graph.world.get::<nodegraph_core::graph::GroupIOKind>(node_id).is_some();
    drop(editor);

    let num_rows = input_ports.len().max(output_ports.len());
    let total_height = HEADER_HEIGHT + num_rows as f64 * PORT_HEIGHT;
    let [hr, hg, hb] = header.color;

    svg!("g", {
        .attr(ATTR_NODE_ID, &format!("{}", node_id.index))
        .attr_signal("transform", pos_signal.signal().map(|(x, y)| format!("translate({}, {})", x, y)))
        .attr("opacity", if is_muted { "0.4" } else { "1" })

        // Double-click to enter group
        .apply(|b| if is_group {
            b.event(clone!(gs, node_id => move |_: events::DoubleClick| {
                gs.enter_group(node_id);
            }))
        } else {
            b
        })

        // Node body background — groups get a distinct border
        .child(svg!("rect", {
            .attr("width", &format!("{}", NODE_MIN_WIDTH))
            .attr("height", &format!("{}", total_height))
            .attr("rx", "6")
            .attr("fill", if is_group { "#2d3d2d" } else { "#2d2d3d" })
            .attr("stroke", if is_group { "#4a7a4a" } else { "none" })
            .attr("stroke-width", if is_group { "1.5" } else { "0" })
            .attr("stroke-dasharray", if is_group { "4,2" } else { "" })
        }))

        // Selection highlight
        .child(svg!("rect", {
            .attr("width", &format!("{}", NODE_MIN_WIDTH))
            .attr("height", &format!("{}", total_height))
            .attr("rx", "6")
            .attr("fill", "none")
            .attr_signal("stroke", {
                let node_id = node_id;
                selection.signal_cloned().map(move |sel| {
                    if sel.contains(&node_id) { "#4a9eff" } else { "none" }
                })
            })
            .attr("stroke-width", "2")
        }))

        // Header background
        .child(svg!("rect", {
            .attr("width", &format!("{}", NODE_MIN_WIDTH))
            .attr("height", &format!("{}", HEADER_HEIGHT))
            .attr("rx", "6")
            .attr("fill", &format!("rgb({},{},{})", hr, hg, hb))
        }))

        // HTML content via foreignObject — title and port labels
        .child(svg!("foreignObject", {
            .attr("x", "0")
            .attr("y", "0")
            .attr("width", &format!("{}", NODE_MIN_WIDTH))
            .attr("height", &format!("{}", total_height))
            .attr("pointer-events", "none")

            .child(html!("div", {
                .attr("xmlns", "http://www.w3.org/1999/xhtml")
                .style("width", "100%")
                .style("height", "100%")
                .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
                .style("user-select", "none")
                .style("pointer-events", "none")

                // Header
                .child(html!("div", {
                    .style("height", &format!("{}px", HEADER_HEIGHT))
                    .style("padding", "0 12px")
                    .style("display", "flex")
                    .style("align-items", "center")
                    .style("color", "white")
                    .style("font-weight", "bold")
                    .style("font-size", "11px")
                    .style("white-space", "nowrap")
                    .style("overflow", "hidden")
                    .style("text-overflow", "ellipsis")
                    .style("box-sizing", "border-box")
                    .text_signal(header_signal.signal_cloned().map(|h| h.title))
                }))

                // Port rows
                .children((0..num_rows).map(|i| {
                    let input_label = input_ports.get(i).map(|(_, _, l, _)| l.clone());
                    let output_label = output_ports.get(i).map(|(_, _, l, _)| l.clone());
                    html!("div", {
                        .style("height", &format!("{}px", PORT_HEIGHT))
                        .style("display", "flex")
                        .style("justify-content", "space-between")
                        .style("align-items", "center")
                        .style("padding", &format!("0 {}px", PORT_RADIUS + 8.0))
                        .style("box-sizing", "border-box")
                        .style("font-size", "10px")
                        .style("color", "#ccc")

                        .child(html!("span", {
                            .text(&input_label.unwrap_or_default())
                        }))
                        .child(html!("span", {
                            .text(&output_label.unwrap_or_default())
                        }))
                    })
                }).collect::<Vec<_>>())
            }))
        }))

        // SVG port circles — at exact layout coordinates (NOT affected by foreignObject)
        .children(input_ports.iter().enumerate().map(|(i, &(pid, st, _, _))| {
            let [cr, cg, cb] = st.default_color();
            let cy = HEADER_HEIGHT + (i as f64 + 0.5) * PORT_HEIGHT;
            render_port(pid, st, PortDirection::Input, 0.0, cy, cr, cg, cb, gs)
        }).collect::<Vec<_>>())

        .children(output_ports.iter().enumerate().map(|(i, &(pid, st, _, _))| {
            let [cr, cg, cb] = st.default_color();
            let cy = HEADER_HEIGHT + (i as f64 + 0.5) * PORT_HEIGHT;
            render_port(pid, st, PortDirection::Output, NODE_MIN_WIDTH, cy, cr, cg, cb, gs)
        }).collect::<Vec<_>>())

        // "Add port" button for Group IO nodes
        .apply(|b| if is_group_io {
            let button_y = total_height + 4.0;
            b.child(svg!("g", {
                .attr("cursor", "pointer")
                .event(clone!(gs, node_id => move |e: events::Click| {
                    e.stop_propagation();
                    gs.select_single(node_id);
                    gs.add_group_io_port();
                }))

                // Button background
                .child(svg!("rect", {
                    .attr("x", &format!("{}", NODE_MIN_WIDTH / 2.0 - 30.0))
                    .attr("y", &format!("{}", button_y))
                    .attr("width", "60")
                    .attr("height", "18")
                    .attr("rx", "9")
                    .attr("fill", "#3a3a5e")
                    .attr("stroke", "#555")
                    .attr("stroke-width", "1")
                }))

                // "+" label
                .child(svg!("text", {
                    .attr("x", &format!("{}", NODE_MIN_WIDTH / 2.0))
                    .attr("y", &format!("{}", button_y + 13.0))
                    .attr("text-anchor", "middle")
                    .attr("fill", "#aaa")
                    .attr("font-size", "12")
                    .attr("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
                    .text("+ port")
                }))
            }))
        } else {
            b
        })
    })
}

fn render_port(
    port_id: EntityId, socket_type: SocketType, direction: PortDirection,
    cx: f64, cy: f64, r: u8, g: u8, b: u8, gs: &Rc<GraphSignals>,
) -> Dom {
    svg!("g", {
        // Invisible larger hit target
        .child(svg!("circle", {
            .attr("cx", &format!("{}", cx))
            .attr("cy", &format!("{}", cy))
            .attr("r", &format!("{}", PORT_RADIUS + 5.0))
            .attr("fill", "transparent")
            .attr("cursor", "crosshair")
            .event_with_options(
                &EventOptions { preventable: true, ..EventOptions::default() },
                clone!(gs, port_id => move |e: events::MouseDown| {
                    e.prevent_default();
                    e.stop_propagation();
                    let screen = Vec2::new(e.mouse_x() as f64, e.mouse_y() as f64);
                    let (pan_x, pan_y) = gs.pan.get();
                    let zoom = gs.zoom.get();
                    let world = Vec2::new((screen.x - pan_x) / zoom, (screen.y - pan_y) / zoom);
                    gs.start_connecting(port_id, screen, world);
                })
            )
        }))

        // Visible port circle at exact layout position
        .child(svg!("circle", {
            .attr(ATTR_PORT_ID, &format!("{}", port_id.index))
            .attr("cx", &format!("{}", cx))
            .attr("cy", &format!("{}", cy))
            .attr("r", &format!("{}", PORT_RADIUS))
            .attr("fill", &format!("rgb({},{},{})", r, g, b))
            .attr("stroke", "rgba(255,255,255,0.3)")
            .attr("stroke-width", "1")
            .attr("pointer-events", "none")

            .attr_signal("transform", {
                let port_id = port_id;
                let socket_type = socket_type;
                let direction = direction;
                map_ref! {
                    let cf = gs.connecting_from.signal_cloned(),
                    let drop = gs.drop_target_port.signal_cloned() => {
                        match cf {
                            Some((src_id, src_type, from_output)) => {
                                let scale = if *src_id == port_id {
                                    1.0
                                } else if drop.as_ref() == Some(&port_id) {
                                    2.0
                                } else if is_valid_connection_target(*from_output, *src_type, direction, socket_type) {
                                    1.4
                                } else {
                                    0.7
                                };
                                if (scale - 1.0_f64).abs() > 0.01 {
                                    format!("translate({}, {}) scale({}) translate({}, {})", cx, cy, scale, -cx, -cy)
                                } else {
                                    String::new()
                                }
                            }
                            None => String::new(),
                        }
                    }
                }
            })
        }))
    })
}
