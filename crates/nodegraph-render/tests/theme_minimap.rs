use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;

use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::types::socket_type::SocketType;

use nodegraph_render::graph_signals::GraphSignals;
use nodegraph_render::viewport_view::render_graph_editor;

wasm_bindgen_test_configure!(run_in_browser);

/// Isolated test container. Removed from DOM on drop.
struct TestContainer {
    element: web_sys::Element,
}

impl TestContainer {
    fn new() -> Self {
        let doc = web_sys::window().unwrap().document().unwrap();
        let el = doc.create_element("div").unwrap();
        el.set_attribute("style", "position:absolute;left:0;top:0;width:800px;height:600px").unwrap();
        doc.body().unwrap().append_child(&el).unwrap();
        Self { element: el }
    }

    fn dom_element(&self) -> web_sys::HtmlElement {
        self.element.clone().dyn_into().unwrap()
    }

    fn query(&self, selector: &str) -> Option<web_sys::Element> {
        self.element.query_selector(selector).unwrap()
    }
}

impl Drop for TestContainer {
    fn drop(&mut self) {
        self.element.remove();
    }
}

fn render_sync(gs: &std::rc::Rc<GraphSignals>) -> TestContainer {
    let container = TestContainer::new();
    dominator::append_dom(&container.dom_element(), render_graph_editor(gs.clone()));
    container
}

async fn flush_microtasks() {
    let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

// ============================================================
// Theme tests
// ============================================================

#[wasm_bindgen_test]
fn test_canvas_uses_theme_bg() {
    let gs = GraphSignals::new();
    let tc = render_sync(&gs);

    // dominator sets styles via JS style property, not style attribute.
    // Check computed style of the editor container.
    let doc = web_sys::window().unwrap().document().unwrap();
    let window = web_sys::window().unwrap();
    let all_divs = doc.query_selector_all("div").unwrap();
    let mut found = false;
    for i in 0..all_divs.length() {
        if let Some(el) = all_divs.get(i) {
            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                let bg = html_el.style().get_property_value("background").unwrap_or_default();
                if bg.contains("26, 26, 46") || bg.contains("1a1a2e") {
                    found = true;
                    break;
                }
            }
        }
    }
    assert!(found, "Should find a div with theme canvas_bg applied via style property");
}

#[wasm_bindgen_test]
async fn test_node_uses_theme_colors() {
    let gs = GraphSignals::new();
    let _n = gs.add_node("Test", (100.0, 100.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let _tc = render_sync(&gs);
    flush_microtasks().await;

    // Query the node body rect — first rect inside [data-node-id] group
    let doc = web_sys::window().unwrap().document().unwrap();
    let node_rect = doc.query_selector("[data-node-id] rect").unwrap();
    assert!(node_rect.is_some(), "Node rect should exist");
    let fill = node_rect.unwrap().get_attribute("fill").unwrap_or_default();
    assert_eq!(fill, gs.theme.node_bg,
        "Node body fill should be theme.node_bg '{}', got '{}'", gs.theme.node_bg, fill);
}

// ============================================================
// Minimap tests
// ============================================================

#[wasm_bindgen_test]
async fn test_minimap_container_exists() {
    let gs = GraphSignals::new();
    gs.add_node("A", (0.0, 0.0), vec![]);
    let _tc = render_sync(&gs);
    flush_microtasks().await;

    let doc = web_sys::window().unwrap().document().unwrap();
    let minimap = doc.query_selector("[data-minimap]").unwrap();
    assert!(minimap.is_some(), "Minimap container should exist in DOM");
}

#[wasm_bindgen_test]
async fn test_minimap_viewport_rect_exists() {
    let gs = GraphSignals::new();
    gs.add_node("A", (0.0, 0.0), vec![]);
    let _tc = render_sync(&gs);
    flush_microtasks().await;

    let doc = web_sys::window().unwrap().document().unwrap();
    let vp_rect = doc.query_selector("[data-minimap-viewport]").unwrap();
    assert!(vp_rect.is_some(), "Minimap viewport rect should exist");
}

#[wasm_bindgen_test]
async fn test_minimap_has_node_rects() {
    let gs = GraphSignals::new();
    gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    gs.add_node("B", (300.0, 200.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let _tc = render_sync(&gs);
    flush_microtasks().await;

    let doc = web_sys::window().unwrap().document().unwrap();
    let minimap = doc.query_selector("[data-minimap] svg").unwrap().unwrap();
    let rects = minimap.query_selector_all("rect").unwrap();
    // Should have at least 2 node rects + 1 viewport rect = 3
    assert!(rects.length() >= 3,
        "Minimap should have at least 3 rects (2 nodes + viewport), got {}", rects.length());
}

#[wasm_bindgen_test]
async fn test_minimap_connection_lines() {
    let gs = GraphSignals::new();
    let n1 = gs.add_node("A", (0.0, 0.0), vec![
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    let n2 = gs.add_node("B", (300.0, 200.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    let (out, inp) = gs.with_graph(|g| (g.node_ports(n1)[0], g.node_ports(n2)[0]));
    gs.connect_ports(out, inp);
    let _tc = render_sync(&gs);
    flush_microtasks().await;

    let doc = web_sys::window().unwrap().document().unwrap();
    let minimap = doc.query_selector("[data-minimap] svg").unwrap().unwrap();
    let lines = minimap.query_selector_all("line").unwrap();
    assert!(lines.length() >= 1,
        "Minimap should have at least 1 connection line, got {}", lines.length());
}
