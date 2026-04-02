use std::rc::Rc;

use dominator::{Dom, svg};
use futures_signals::signal::{Mutable, SignalExt};

use nodegraph_core::store::EntityId;

use crate::graph_signals::GraphSignals;

pub fn render_connection(conn_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let path_signal = gs.get_connection_path_signal(conn_id)
        .unwrap_or_else(|| Mutable::new(String::new()));

    svg!("path", {
        .attr_signal("d", path_signal.signal_cloned())
        .attr("fill", "none")
        .attr("stroke", "#aaa")
        .attr("stroke-width", "2")
    })
}

pub fn render_preview_wire(gs: &Rc<GraphSignals>) -> Dom {
    let preview = gs.preview_wire.clone();

    svg!("path", {
        .attr_signal("d", preview.signal_cloned().map(|opt| {
            opt.map(|p| p.to_svg_d()).unwrap_or_default()
        }))
        .attr("fill", "none")
        .attr("stroke", "#4a9eff")
        .attr("stroke-width", "2")
        .attr("stroke-dasharray", "6,4")
        .attr_signal("visibility", preview.signal_cloned().map(|opt| {
            if opt.is_some() { "visible" } else { "hidden" }
        }))
    })
}
