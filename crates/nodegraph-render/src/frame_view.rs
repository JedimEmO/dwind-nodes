use std::rc::Rc;

use dominator::{html, Dom, svg};
use futures_signals::signal::{Mutable, SignalExt};

use nodegraph_core::graph::frame::{FrameLabel, FrameColor};
use nodegraph_core::store::EntityId;

use crate::graph_signals::GraphSignals;

pub fn render_frame(frame_id: EntityId, gs: &Rc<GraphSignals>) -> Dom {
    let (label, color) = gs.with_graph(|g| {
        let label = g.world.get::<FrameLabel>(frame_id).map(|l| l.0.clone())
            .unwrap_or_default();
        let color = g.world.get::<FrameColor>(frame_id).map(|c| c.0)
            .unwrap_or([80, 80, 120]);
        (label, color)
    });

    let [r, g, b] = color;

    let bounds = gs.get_frame_bounds_signal(frame_id)
        .unwrap_or_else(|| Mutable::new((0.0, 0.0, 200.0, 100.0)));

    let is_selected = gs.selected_frames.signal_cloned()
        .map(move |sel| sel.contains(&frame_id))
        .broadcast();

    svg!("g", {
        .child(svg!("rect", {
            .attr_signal("x", bounds.signal().map(|(x, _, _, _)| format!("{}", x)))
            .attr_signal("y", bounds.signal().map(|(_, y, _, _)| format!("{}", y)))
            .attr_signal("width", bounds.signal().map(|(_, _, w, _)| format!("{}", w)))
            .attr_signal("height", bounds.signal().map(|(_, _, _, h)| format!("{}", h)))
            .attr("rx", "8")
            .attr("fill", &format!("rgba({},{},{},{})", r, g, b, gs.theme.frame_fill_opacity))
            .attr_signal("stroke", {
                let sel_opacity = gs.theme.frame_selected_opacity;
                let norm_opacity = gs.theme.frame_stroke_opacity;
                is_selected.signal().map(move |sel| {
                    let opacity = if sel { sel_opacity } else { norm_opacity };
                    format!("rgba({},{},{},{})", r, g, b, opacity)
                })
            })
            .attr_signal("stroke-width", is_selected.signal().map(|sel| {
                if sel { "2" } else { "1" }
            }))
            .attr("stroke-dasharray", "6,3")
        }))

        .child(svg!("foreignObject", {
            .attr_signal("x", bounds.signal().map(|(x, _, _, _)| format!("{}", x)))
            .attr_signal("y", bounds.signal().map(|(_, y, _, _)| format!("{}", y - 20.0)))
            .attr_signal("width", bounds.signal().map(|(_, _, w, _)| format!("{}", w)))
            .attr("height", "20")
            .attr("pointer-events", "none")

            .child(html!("div", {
                .attr("xmlns", "http://www.w3.org/1999/xhtml")
                .style("color", &format!("rgb({},{},{})", r, g, b))
                .style("font-size", "11px")
                .style("font-weight", "bold")
                .style("font-family", "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif")
                .style("padding-left", "8px")
                .text(&label)
            }))
        }))
    })
}
