//! Trivial Calculator — dwind-nodes example with inline port widgets.
//!
//! Three node types: Constant, Add, and Display.
//! Input ports show editable float fields. The graph evaluates reactively.
//!
//! Run with: `trunk serve` from this directory.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use dominator::{html, clone};
use futures_signals::signal::{Mutable, SignalExt};
use futures_signals::signal_vec::SignalVecExt;

use nodegraph_core::graph::node::NodeHeader;
use nodegraph_core::graph::port::{PortDirection, PortOwner};
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::search::{NodeTypeDefinition, PortDefinition};
use nodegraph_core::store::EntityId;
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_render::graph_signals::GraphSignals;
use nodegraph_render::viewport_view::render_graph_editor;
use nodegraph_widgets::float_input::{float_input, FloatInputProps};

// ============================================================
// Shared reactive value store — one Mutable<f64> per port
// ============================================================

type PortValues = Rc<RefCell<HashMap<EntityId, Mutable<f64>>>>;

fn get_port_value(values: &PortValues, port_id: EntityId, default: f64) -> Mutable<f64> {
    values.borrow_mut()
        .entry(port_id)
        .or_insert_with(|| Mutable::new(default))
        .clone()
}

// ============================================================
// Graph evaluation
// ============================================================

fn evaluate(gs: &Rc<GraphSignals>, port_values: &PortValues) {
    let mut computed: HashMap<EntityId, f64> = HashMap::new();

    gs.with_graph(|graph| {
        // Pass 1: Constant nodes — read value from their output port's Mutable
        for (id, header) in graph.world.query::<NodeHeader>() {
            if header.title != "Constant" { continue; }
            let ports = graph.node_ports(id);
            if let Some(&out_port) = ports.first() {
                let val = get_port_value(port_values, out_port, 0.0).get();
                computed.insert(id, val);
            }
        }

        // Pass 2: Add nodes — sum connected inputs, fall back to port widget value
        let add_nodes: Vec<EntityId> = graph.world.query::<NodeHeader>()
            .filter(|(_, h)| h.title == "Add")
            .map(|(id, _)| id).collect();

        for nid in add_nodes {
            let mut sum = 0.0;
            for &pid in graph.node_ports(nid) {
                if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }

                let conns = graph.port_connections(pid);
                if conns.is_empty() {
                    // No connection — use the port widget's value
                    sum += get_port_value(port_values, pid, 0.0).get();
                } else {
                    // Connected — use upstream computed value
                    for &conn_id in conns {
                        if let Some(ep) = graph.world.get::<ConnectionEndpoints>(conn_id) {
                            if ep.target_port != pid { continue; }
                            if let Some(src_node) = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0) {
                                sum += computed.get(&src_node).copied().unwrap_or(0.0);
                            }
                        }
                    }
                }
            }
            computed.insert(nid, sum);
        }

        // Pass 3: Display nodes — take connected input value
        let display_nodes: Vec<EntityId> = graph.world.query::<NodeHeader>()
            .filter(|(_, h)| h.title == "Display")
            .map(|(id, _)| id).collect();

        for nid in display_nodes {
            let mut val = 0.0;
            for &pid in graph.node_ports(nid) {
                if graph.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }
                for &conn_id in graph.port_connections(pid) {
                    if let Some(ep) = graph.world.get::<ConnectionEndpoints>(conn_id) {
                        if ep.target_port != pid { continue; }
                        if let Some(src_node) = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0) {
                            val = computed.get(&src_node).copied().unwrap_or(0.0);
                        }
                    }
                }
            }
            computed.insert(nid, val);
        }
    });

    // Push results to display Mutables
    for (node_id, val) in &computed {
        // Update the display value Mutable (used by custom_node_body)
        get_port_value(port_values, *node_id, 0.0).set(*val);
    }
}

// ============================================================
// Entry point
// ============================================================

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    dwind::stylesheet();

    let gs = GraphSignals::new();
    let port_values: PortValues = Rc::new(RefCell::new(HashMap::new()));

    // Register node types
    {
        let mut reg = gs.registry.borrow_mut();
        reg.register(NodeTypeDefinition {
            type_id: "constant".into(),
            display_name: "Constant".into(),
            category: "Input".into(),
            input_ports: vec![],
            output_ports: vec![
                PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Value".into() },
            ],
        });
        reg.register(NodeTypeDefinition {
            type_id: "add".into(),
            display_name: "Add".into(),
            category: "Math".into(),
            input_ports: vec![
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "A".into() },
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "B".into() },
            ],
            output_ports: vec![
                PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Result".into() },
            ],
        });
        reg.register(NodeTypeDefinition {
            type_id: "display".into(),
            display_name: "Display".into(),
            category: "Output".into(),
            input_ports: vec![
                PortDefinition { direction: PortDirection::Input, socket_type: SocketType::Float, label: "Value".into() },
            ],
            output_ports: vec![],
        });
    }

    // Add nodes
    let (const_a, const_a_ports) = gs.add_node("Constant", (50.0, 50.0), vec![
        (PortDirection::Output, SocketType::Float, "Value".to_string()),
    ]);
    let (const_b, const_b_ports) = gs.add_node("Constant", (50.0, 200.0), vec![
        (PortDirection::Output, SocketType::Float, "Value".to_string()),
    ]);
    let (add, add_ports) = gs.add_node("Add", (300.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "A".to_string()),
        (PortDirection::Input, SocketType::Float, "B".to_string()),
        (PortDirection::Output, SocketType::Float, "Result".to_string()),
    ]);
    let (display, display_ports) = gs.add_node("Display", (550.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "Value".to_string()),
    ]);

    // Set initial constant values
    get_port_value(&port_values, const_a_ports[0], 0.0).set(42.0);
    get_port_value(&port_values, const_b_ports[0], 0.0).set(8.0);

    // Connect: Constant(42) → Add.A, Constant(8) → Add.B, Add.Result → Display
    gs.connect_ports(const_a_ports[0], add_ports[0]).unwrap();
    gs.connect_ports(const_b_ports[0], add_ports[1]).unwrap();
    gs.connect_ports(add_ports[2], display_ports[0]).unwrap();

    // Port widget callback — renders float_input on Float ports
    {
        let pv = port_values.clone();
        gs.port_widget.borrow_mut().replace(Rc::new(move |_node_id, port_id, socket_type, port_dir, type_id, is_connected, _gs| {
            if socket_type != SocketType::Float { return None; }
            match (type_id, port_dir) {
                ("constant", PortDirection::Output) => {} // Always editable
                ("display", _) => return None,            // Display never gets widgets
                (_, PortDirection::Input) if !is_connected => {} // Disconnected inputs get widgets
                _ => return None,
            }
            let mutable = get_port_value(&pv, port_id, 0.0);
            Some(float_input(FloatInputProps::new()
                .value(Box::new(mutable) as Box<dyn nodegraph_widgets::FloatValueWrapper>)
            ))
        }));
    }

    // Custom node body — shows computed result
    // Display nodes: show value or "[disconnected]"
    // Other nodes: show "= value"
    {
        let pv = port_values.clone();
        gs.custom_node_body.borrow_mut().replace(Rc::new(move |node_id, gs| {
            let title = gs.with_graph(|g| {
                g.world.get::<NodeHeader>(node_id).map(|h| h.title.clone()).unwrap_or_default()
            });

            match title.as_str() {
                "Display" => {
                    let gs = gs.clone();
                    let display_mutable = get_port_value(&pv, node_id, 0.0);
                    Some(html!("div", {
                        .attr("xmlns", "http://www.w3.org/1999/xhtml")
                        .style("padding", "2px 14px")
                        .style("font-size", "14px")
                        .style("font-weight", "bold")
                        .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
                        .style("text-align", "center")
                        // React to both value changes and connection changes
                        .child_signal(
                            futures_signals::map_ref! {
                                let val = display_mutable.signal(),
                                let _conns = gs.connection_list.signal_vec_cloned().to_signal_cloned() => {
                                    (*val, ())
                                }
                            }.map(move |(val, _)| {
                                let has_input = gs.with_graph(|g| {
                                    g.node_ports(node_id).iter().any(|&pid| {
                                        g.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Input)
                                            && !g.port_connections(pid).is_empty()
                                    })
                                });
                                if has_input {
                                    Some(html!("span", {
                                        .style("color", "#4a9eff")
                                        .text(&format!("{}", val))
                                    }))
                                } else {
                                    Some(html!("span", {
                                        .style("color", "#666")
                                        .style("font-size", "11px")
                                        .text("[disconnected]")
                                    }))
                                }
                            })
                        )
                    }))
                }
                "Constant" => None,
                _ => None,
            }
        }));
    }

    // Initial evaluation
    evaluate(&gs, &port_values);

    // Re-evaluate reactively on connection changes
    {
        let pv = port_values.clone();
        wasm_bindgen_futures::spawn_local(clone!(gs => async move {
            gs.connection_list.signal_vec_cloned()
                .to_signal_cloned()
                .for_each(clone!(gs => move |_| {
                    evaluate(&gs, &pv);
                    async {}
                })).await;
        }));
    }

    // Re-evaluate when any input port value changes (e.g., user edits a Constant or Add default)
    {
        let pv = port_values.clone();
        let all_input_ports: Vec<EntityId> = {
            let editor = gs.editor.borrow();
            let graph = editor.current_graph();
            let mut ports = Vec::new();
            for (nid, _) in graph.world.query::<NodeHeader>() {
                for &pid in graph.node_ports(nid) {
                    ports.push(pid);
                }
            }
            ports
        };
        for port_id in all_input_ports {
            let pv = pv.clone();
            let gs = gs.clone();
            let mutable = get_port_value(&pv, port_id, 0.0);
            wasm_bindgen_futures::spawn_local(async move {
                mutable.signal().for_each(move |_| {
                    evaluate(&gs, &pv);
                    async {}
                }).await;
            });
        }
    }

    // Render
    dominator::append_dom(&dominator::body(), render_graph_editor(gs));
}
