use std::rc::Rc;

use dominator::{html, Dom, clone, svg, events, EventOptions};
use futures_signals::signal::{Mutable, SignalExt};
use futures_signals::signal_vec::SignalVecExt;
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
    let io_kind = graph.world.get::<nodegraph_core::graph::GroupIOKind>(node_id).cloned();
    let is_reroute = graph.world.get::<nodegraph_core::graph::reroute::IsReroute>(node_id).is_some();
    drop(editor);

    // Group IO nodes render as small labeled rects
    if let Some(kind) = io_kind {
        return render_group_io(node_id, pos_signal, selection, &input_ports, &output_ports, kind, gs);
    }

    // Reroute nodes render as a small diamond
    if is_reroute {
        return render_reroute(node_id, pos_signal, selection, &input_ports, &output_ports, gs);
    }


    let collapsed = header.collapsed;
    let num_rows = if collapsed { 0 } else { input_ports.len().max(output_ports.len()) };
    let has_custom_body = gs.custom_node_body.borrow().is_some();
    let custom_body_height = if has_custom_body { PORT_HEIGHT } else { 0.0 };
    let total_height = HEADER_HEIGHT + num_rows as f64 * PORT_HEIGHT + custom_body_height;
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
            .attr("fill", if is_group { gs.theme.group_node_bg } else { gs.theme.node_bg })
            .attr("stroke", if is_group { gs.theme.group_node_border } else { "none" })
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
                let highlight = gs.theme.selection_highlight;
                selection.signal_cloned().map(move |sel| {
                    if sel.contains(&node_id) { highlight } else { "none" }
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
                    .style("padding", &format!("0 {}px", PORT_RADIUS + 8.0))
                    .style("display", "flex")
                    .style("align-items", "center")
                    .style("color", gs.theme.header_text)
                    .style("font-weight", "bold")
                    .style("font-size", "11px")
                    .style("white-space", "nowrap")
                    .style("overflow", "hidden")
                    .style("text-overflow", "ellipsis")
                    .style("box-sizing", "border-box")
                    .text_signal(header_signal.signal_cloned().map(|h| h.title))
                }))

                // Port rows
                .children({
                    let port_widget = gs.port_widget.borrow().clone();
                    let node_type_id: String = gs.with_graph(|g| {
                        g.world.get::<nodegraph_core::graph::node::NodeTypeId>(node_id)
                            .map(|t| t.0.clone()).unwrap_or_default()
                    });
                    (0..num_rows).map(|i| {
                        let input_info = input_ports.get(i).map(|(pid, st, l, _)| (*pid, *st, l.clone()));
                        let output_info = output_ports.get(i).map(|(pid, st, l, _)| (*pid, *st, l.clone()));
                        let input_label = input_info.as_ref().map(|(_, _, l)| l.clone()).unwrap_or_default();
                        let output_label = output_info.as_ref().map(|(_, _, l)| l.clone()).unwrap_or_default();

                        html!("div", {
                            .style("height", &format!("{}px", PORT_HEIGHT))
                            .style("display", "flex")
                            .style("justify-content", "space-between")
                            .style("align-items", "center")
                            .style("padding", &format!("0 {}px", PORT_RADIUS + 8.0))
                            .style("box-sizing", "border-box")
                            .style("font-size", "10px")
                            .style("color", gs.theme.port_label_text)
                            .style("gap", "3px")

                            // Left side: input label + input widget
                            .child(html!("span", {
                                .style("flex-shrink", "0")
                                .text(&input_label)
                            }))
                            .child_signal({
                                let port_widget = port_widget.clone();
                                let input_info = input_info.clone();
                                let gs = gs.clone();
                                let nti = node_type_id.clone();
                                gs.connection_list.signal_vec_cloned()
                                    .to_signal_cloned()
                                    .map(move |_| {
                                        input_info.as_ref().and_then(|(pid, st, _)| {
                                            let pw = port_widget.as_ref()?;
                                            let is_connected = gs.with_graph(|g| !g.port_connections(*pid).is_empty());
                                            pw(node_id, *pid, *st, PortDirection::Input, &nti, is_connected, &gs)
                                        }).map(|dom| html!("div", {
                                            .style("width", "50px")
                                            .style("flex-shrink", "0")
                                            .child(dom)
                                        }))
                                    })
                            })

                            // Spacer
                            .child(html!("span", { .style("flex", "1") }))

                            // Right side: output widget + output label
                            .child_signal({
                                let port_widget = port_widget.clone();
                                let output_info = output_info.clone();
                                let gs = gs.clone();
                                let nti = node_type_id.clone();
                                gs.connection_list.signal_vec_cloned()
                                    .to_signal_cloned()
                                    .map(move |_| {
                                        output_info.as_ref().and_then(|(pid, st, _)| {
                                            let pw = port_widget.as_ref()?;
                                            let is_connected = gs.with_graph(|g| !g.port_connections(*pid).is_empty());
                                            pw(node_id, *pid, *st, PortDirection::Output, &nti, is_connected, &gs)
                                        }).map(|dom| html!("div", {
                                            .style("width", "50px")
                                            .style("flex-shrink", "0")
                                            .child(dom)
                                        }))
                                    })
                            })
                            .child(html!("span", {
                                .style("flex-shrink", "0")
                                .text(&output_label)
                            }))
                        })
                    }).collect::<Vec<_>>()
                })

                // Custom node body — user-defined content inside the node
                .apply({
                    let custom = gs.custom_node_body.borrow().clone();
                    move |b| {
                        if let Some(renderer) = custom {
                            if let Some(dom) = renderer(node_id, gs) {
                                return b.child(dom);
                            }
                        }
                        b
                    }
                })
            }))
        }))

        // SVG port circles — hidden when collapsed
        .children(if collapsed { vec![] } else {
            input_ports.iter().enumerate().map(|(i, &(pid, st, _, _))| {
                let [cr, cg, cb] = st.default_color();
                let cy = HEADER_HEIGHT + (i as f64 + 0.5) * PORT_HEIGHT;
                render_port(pid, st, PortDirection::Input, 0.0, cy, cr, cg, cb, gs)
            }).collect::<Vec<_>>()
        })

        .children(if collapsed { vec![] } else {
            output_ports.iter().enumerate().map(|(i, &(pid, st, _, _))| {
                let [cr, cg, cb] = st.default_color();
                let cy = HEADER_HEIGHT + (i as f64 + 0.5) * PORT_HEIGHT;
                render_port(pid, st, PortDirection::Output, NODE_MIN_WIDTH, cy, cr, cg, cb, gs)
            }).collect::<Vec<_>>()
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
            .attr("stroke", gs.theme.port_stroke)
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

fn render_reroute(
    node_id: EntityId,
    pos_signal: Mutable<(f64, f64)>,
    selection: Mutable<Vec<EntityId>>,
    input_ports: &[(EntityId, SocketType, String, u32)],
    output_ports: &[(EntityId, SocketType, String, u32)],
    gs: &Rc<GraphSignals>,
) -> Dom {
    let size = nodegraph_core::layout::REROUTE_SIZE;
    // Diamond shape centered at (0, 0) relative to node position
    let diamond = format!("{},0 0,{} -{},0 0,-{}", size, size, size, size);

    svg!("g", {
        .attr(ATTR_NODE_ID, &format!("{}", node_id.index))
        .attr_signal("transform", pos_signal.signal().map(|(x, y)| format!("translate({}, {})", x, y)))

        // Diamond shape
        .child(svg!("polygon", {
            .attr("points", &diamond)
            .attr("fill", gs.theme.reroute_fill)
            .attr("stroke", gs.theme.reroute_stroke)
            .attr("stroke-width", "1.5")
        }))

        // Selection highlight
        .child(svg!("polygon", {
            .attr("points", &diamond)
            .attr("fill", "none")
            .attr_signal("stroke", {
                let node_id = node_id;
                let highlight = gs.theme.selection_highlight;
                selection.signal_cloned().map(move |sel| {
                    if sel.contains(&node_id) { highlight } else { "none" }
                })
            })
            .attr("stroke-width", "2")
        }))

        // Port hit targets (input on left, output on right)
        .children(input_ports.iter().map(|&(pid, st, _, _)| {
            render_port(pid, st, PortDirection::Input, -size, 0.0, 200, 200, 200, gs)
        }).collect::<Vec<_>>())

        .children(output_ports.iter().map(|&(pid, st, _, _)| {
            render_port(pid, st, PortDirection::Output, size, 0.0, 200, 200, 200, gs)
        }).collect::<Vec<_>>())
    })
}

fn render_group_io(
    node_id: EntityId,
    pos_signal: Mutable<(f64, f64)>,
    selection: Mutable<Vec<EntityId>>,
    input_ports: &[(EntityId, SocketType, String, u32)],
    output_ports: &[(EntityId, SocketType, String, u32)],
    io_kind: nodegraph_core::graph::GroupIOKind,
    gs: &Rc<GraphSignals>,
) -> Dom {
    use nodegraph_core::layout::{IO_NODE_WIDTH, IO_NODE_HEIGHT};
    use nodegraph_core::graph::GroupIOKind;

    let is_input = matches!(io_kind, GroupIOKind::Input);

    let title = gs.get_node_header_signal(node_id)
        .map(|m| m.get_cloned().title)
        .unwrap_or_else(|| "IO".to_string());

    // Get the socket type color from the single port
    let port_color = if is_input {
        output_ports.first().map(|(_, st, _, _)| st.default_color())
    } else {
        input_ports.first().map(|(_, st, _, _)| st.default_color())
    }.unwrap_or([160, 160, 160]);

    let [pr, pg, pb] = port_color;
    let bg_color = if is_input { gs.theme.io_node_input_bg } else { gs.theme.io_node_output_bg };
    let highlight = gs.theme.selection_highlight;

    svg!("g", {
        .attr(ATTR_NODE_ID, &format!("{}", node_id.index))
        .attr_signal("transform", pos_signal.signal().map(|(x, y)| format!("translate({}, {})", x, y)))

        // Background rect
        .child(svg!("rect", {
            .attr("width", &format!("{}", IO_NODE_WIDTH))
            .attr("height", &format!("{}", IO_NODE_HEIGHT))
            .attr("rx", "4")
            .attr("fill", bg_color)
            .attr("stroke", &format!("rgb({},{},{})", pr, pg, pb))
            .attr("stroke-width", "1.5")
        }))

        // Selection highlight
        .child(svg!("rect", {
            .attr("width", &format!("{}", IO_NODE_WIDTH))
            .attr("height", &format!("{}", IO_NODE_HEIGHT))
            .attr("rx", "4")
            .attr("fill", "none")
            .attr_signal("stroke", {
                let node_id = node_id;
                selection.signal_cloned().map(move |sel| {
                    if sel.contains(&node_id) { highlight } else { "none" }
                })
            })
            .attr("stroke-width", "2")
        }))

        // Label text
        .child(svg!("text", {
            .attr("x", &format!("{}", IO_NODE_WIDTH / 2.0))
            .attr("y", &format!("{}", IO_NODE_HEIGHT / 2.0 + 4.0))
            .attr("text-anchor", "middle")
            .attr("fill", gs.theme.io_node_text)
            .attr("font-size", "10")
            .attr("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
            .attr("pointer-events", "none")
            .text(&title)
        }))

        // Port circle — on right edge for Input IO, left edge for Output IO
        .children(if is_input {
            output_ports.iter().map(|&(pid, st, _, _)| {
                let [cr, cg, cb] = st.default_color();
                render_port(pid, st, PortDirection::Output, IO_NODE_WIDTH, IO_NODE_HEIGHT / 2.0, cr, cg, cb, gs)
            }).collect::<Vec<_>>()
        } else {
            input_ports.iter().map(|&(pid, st, _, _)| {
                let [cr, cg, cb] = st.default_color();
                render_port(pid, st, PortDirection::Input, 0.0, IO_NODE_HEIGHT / 2.0, cr, cg, cb, gs)
            }).collect::<Vec<_>>()
        })
    })
}
