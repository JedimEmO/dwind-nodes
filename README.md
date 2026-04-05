# dwind-nodes

Blender-style node graph editor for Rust/WASM, built on [dwind](https://github.com/JedimEmO/dwind)/[dominator](https://github.com/nicksenger/dominator)/[futures-signals](https://github.com/nicksenger/futures-signals).

**[Live Demo](https://mathiasmyrland.github.io/dwind-nodes/)** — Interactive calculator graph with inline editing

## Features

- **Pure SVG rendering** with foreignObject for HTML node content
- **Full interaction suite**: drag nodes, connect ports, box select, cut links, pan/zoom
- **Node groups** with subgraph navigation and breadcrumb trail
- **Frames** for visual grouping with drag-to-move, rename, and color picker
- **Reroute nodes** for wire management (diamond-shaped pass-through)
- **Search menu** (Shift+A) with type-compatible filtering and noodle-drop-to-add
- **Right-click context menu** with per-target actions
- **Minimap** with click-to-pan
- **Theme system** with centralized color configuration
- **Snapshot-based undo/redo** (Ctrl+Z / Ctrl+Shift+Z)
- **Topological sort** for dependency-ordered graph evaluation
- **Full serialization** including subgraph hierarchy (JSON-compatible)
- **Inline port widgets** for editing values when ports are disconnected
- **Auto-insert on wire**: drag a node onto a connection to splice it in
- **Auto-reconnect on delete**: removing a node bridges compatible neighbors
- **Event callbacks**: on_connect, on_disconnect, on_selection_changed, on_node_moved

## Crates

| Crate | Description |
|-------|-------------|
| `nodegraph-core` | Graph data model, ECS store, interaction, layout, serialization |
| `nodegraph-render` | SVG rendering, reactive signals, UI components |
| `nodegraph-widgets` | Compact inline input widgets (float, int, bool, string) |

## Quick Start

```rust
use nodegraph_core::{SocketType, PortDirection, NodeTypeDefinition, PortDefinition};
use nodegraph_render::{GraphSignals, render_graph_editor};

// 1. Create the editor
let gs = GraphSignals::new();

// 2. Register node types (appear in Shift+A search menu)
gs.registry.borrow_mut().register(NodeTypeDefinition {
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

// 3. Add nodes — returns (node_id, port_ids)
let (node_a, ports_a) = gs.add_node_typed("Constant", Some("constant"), (50.0, 100.0), vec![
    (PortDirection::Output, SocketType::Float, "Value".to_string()),
]);

let (node_b, ports_b) = gs.add_node_typed("Add", Some("add"), (300.0, 100.0), vec![
    (PortDirection::Input, SocketType::Float, "A".to_string()),
    (PortDirection::Input, SocketType::Float, "B".to_string()),
    (PortDirection::Output, SocketType::Float, "Result".to_string()),
]);

// 4. Connect ports — returns Result with typed errors
gs.connect_ports(ports_a[0], ports_b[0]).unwrap();

// 5. Render
dominator::append_dom(&dominator::body(), render_graph_editor(gs));
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Shift+A | Open search menu (add node) |
| Delete / X | Delete selected |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Shift+D | Duplicate selected |
| G | Group selected nodes |
| Shift+G | Ungroup |
| F | Create frame around selected |
| H | Toggle collapse |
| M | Toggle mute |
| A | Select all / Deselect all |
| Middle mouse | Pan |
| Scroll | Zoom |
| Ctrl+RMB drag | Cut links |

## Graph Evaluation

The library provides graph structure and rendering — evaluation is user-defined. Use `NodeGraph::topological_sort()` for dependency ordering:

```rust
gs.with_graph(|graph| {
    for node_id in graph.eval_order() {
        // Process nodes in dependency order
    }
});
```

React to changes via event callbacks:

```rust
*gs.on_connect.borrow_mut() = Some(Box::new(|src, tgt, conn_id| {
    // Re-evaluate the graph
}));
```

See [`examples/trivial-calculator`](examples/trivial-calculator/) for a complete working example with inline widgets and reactive evaluation.

## Building

Requires [Trunk](https://trunkrs.dev/) for WASM builds:

```bash
# Run the demo
cd crates/nodegraph-demo && trunk serve

# Run the calculator example
cd examples/trivial-calculator && trunk serve

# Run tests
cargo test -p nodegraph-core
wasm-pack test --headless --firefox crates/nodegraph-render
```

## License

MIT
