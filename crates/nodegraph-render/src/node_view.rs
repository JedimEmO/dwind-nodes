use std::rc::Rc;

use dominator::{html, Dom, clone};
use futures_signals::signal::{Mutable, SignalExt};

use nodegraph_core::graph::node::NodeHeader;
use nodegraph_core::graph::port::{PortDirection, PortSocketType, PortLabel};
use nodegraph_core::layout::{NODE_MIN_WIDTH, PORT_RADIUS};
use nodegraph_core::store::EntityId;
use nodegraph_core::types::socket_type::SocketType;

use crate::graph_signals::GraphSignals;

pub fn render_node(node_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let pos_signal = gs.get_node_position_signal(node_id)
        .unwrap_or_else(|| Mutable::new((0.0, 0.0)));
    let header_signal = gs.get_node_header_signal(node_id)
        .unwrap_or_else(|| Mutable::new(NodeHeader {
            title: "?".to_string(),
            color: [100, 100, 100],
            collapsed: false,
        }));

    let selection = gs.selection.clone();

    // Get port info from graph
    let graph = gs.graph.borrow();
    let ports = graph.node_ports(node_id).to_vec();
    let mut input_ports = Vec::new();
    let mut output_ports = Vec::new();
    for &pid in &ports {
        let dir = graph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Input);
        let st = graph.world.get::<PortSocketType>(pid).map(|s| s.0).unwrap_or(SocketType::Float);
        let label = graph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
        match dir {
            PortDirection::Input => input_ports.push((pid, st, label)),
            PortDirection::Output => output_ports.push((pid, st, label)),
        }
    }
    let header = graph.world.get::<NodeHeader>(node_id).cloned()
        .unwrap_or(NodeHeader { title: "?".to_string(), color: [100, 100, 100], collapsed: false });
    drop(graph);

    let num_rows = input_ports.len().max(output_ports.len());
    let [r, g, b] = header.color;

    html!("div", {
        .attr("data-node-id", &format!("{}", node_id.index))
        .style("position", "absolute")
        .style("width", &format!("{}px", NODE_MIN_WIDTH))
        .style("border-radius", "6px")
        .style("overflow", "visible")
        .style("box-shadow", "0 2px 8px rgba(0,0,0,0.5)")
        .style("user-select", "none")
        .style("cursor", "default")
        .style("font-size", "12px")

        // Reactive position
        .style_signal("left", pos_signal.signal().map(|(x, _)| format!("{}px", x)))
        .style_signal("top", pos_signal.signal().map(|(_, y)| format!("{}px", y)))

        // Selection border
        .style_signal("outline", {
            let node_id = node_id;
            selection.signal_cloned().map(move |sel| {
                if sel.contains(&node_id) {
                    "2px solid #4a9eff".to_string()
                } else {
                    "none".to_string()
                }
            })
        })

        // Header — natural height from content
        .child(html!("div", {
            .style("background", &format!("rgb({},{},{})", r, g, b))
            .style("padding", "6px 12px")
            .style("color", "white")
            .style("font-weight", "bold")
            .style("font-size", "11px")
            .style("white-space", "nowrap")
            .style("overflow", "hidden")
            .style("text-overflow", "ellipsis")
            .style("border-radius", "6px 6px 0 0")
            .text_signal(header_signal.signal_cloned().map(|h| h.title))
        }))

        // Body — port rows, natural height from content
        .child(html!("div", {
            .style("background", "#2d2d3d")
            .style("border-radius", "0 0 6px 6px")
            .style("padding", "4px 0")
            .children({
                (0..num_rows).map(|i| {
                    let input = input_ports.get(i).cloned();
                    let output = output_ports.get(i).cloned();
                    render_port_row(input, output, gs)
                }).collect::<Vec<_>>()
            })
        }))
    })
}

fn render_port_row(
    input: Option<(EntityId, SocketType, String)>,
    output: Option<(EntityId, SocketType, String)>,
    gs: &Rc<GraphSignals>,
) -> Dom {
    html!("div", {
        .style("display", "flex")
        .style("justify-content", "space-between")
        .style("align-items", "center")
        .style("padding", "3px 0")

        // Input port (left)
        .child(html!("div", {
            .style("display", "flex")
            .style("align-items", "center")
            .style("gap", "6px")
            .apply(|dom| if let Some((pid, st, label)) = input {
                let [cr, cg, cb] = st.default_color();
                dom.child(render_port_circle(pid, cr, cg, cb, gs))
                   .child(html!("span", {
                       .style("color", "#ccc")
                       .style("font-size", "10px")
                       .text(&label)
                   }))
            } else {
                dom
            })
        }))

        // Output port (right)
        .child(html!("div", {
            .style("display", "flex")
            .style("align-items", "center")
            .style("gap", "6px")
            .apply(|dom| if let Some((pid, st, label)) = output {
                let [cr, cg, cb] = st.default_color();
                dom.child(html!("span", {
                       .style("color", "#ccc")
                       .style("font-size", "10px")
                       .text(&label)
                   }))
                   .child(render_port_circle(pid, cr, cg, cb, gs))
            } else {
                dom
            })
        }))
    })
}

fn render_port_circle(port_id: EntityId, r: u8, g: u8, b: u8, gs: &Rc<GraphSignals>) -> Dom {
    let diameter = PORT_RADIUS * 2.0;
    html!("div", {
        .attr("data-port-id", &format!("{}", port_id.index))
        .style("width", &format!("{}px", diameter))
        .style("height", &format!("{}px", diameter))
        .style("border-radius", "50%")
        .style("background", &format!("rgb({},{},{})", r, g, b))
        .style("border", "1px solid rgba(255,255,255,0.3)")
        .style("flex-shrink", "0")

        // Measure the port circle's offset relative to its ancestor node div.
        // This offset is stable — only the node position changes when dragging.
        .after_inserted(clone!(gs => move |el| {
            let port_rect = el.get_bounding_client_rect();
            let zoom = gs.zoom.get();

            // Walk up to the node div (has data-node-id attribute)
            let mut node_el = el.parent_element();
            while let Some(ref parent) = node_el {
                if parent.has_attribute("data-node-id") {
                    break;
                }
                node_el = parent.parent_element();
            }

            if let Some(node_el) = node_el {
                let node_rect = node_el.get_bounding_client_rect();
                // getBoundingClientRect returns screen-scaled values.
                // Divide by zoom to get the offset in world/CSS space.
                let offset_x = (port_rect.left() + port_rect.width() / 2.0 - node_rect.left()) / zoom;
                let offset_y = (port_rect.top() + port_rect.height() / 2.0 - node_rect.top()) / zoom;
                gs.report_port_offset(port_id, offset_x, offset_y);
            }
        }))
    })
}
