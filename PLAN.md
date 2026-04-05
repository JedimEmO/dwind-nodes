# dwind-nodes — Implementation Plan

Blender-style node graph UI library built on dwind/dominator/futures-signals (Rust/WASM).

## Architecture

- **nodegraph-core**: Custom ECS store, graph model, interaction state machine, layout, serialization, search registry, topological sort
- **nodegraph-render**: SVG rendering via dominator, reactive signal bridge (GraphSignals), event handling, search menu UI, minimap, context menu
- **nodegraph-widgets**: Compact inline input components (`float_input`, etc.) using `#[component]` macro
- **nodegraph-demo**: Demo app with Blender-style node types
- **examples/trivial-calculator**: Minimal example with graph evaluation, inline widgets, and live JSON panel

### Core Principles
- Pure FRP: DOM as computation over mutable state, no imperative sync
- SVG rendering for graph structure, foreignObject for HTML node content
- Snapshot-based undo (full GraphEditor clone, not command pattern)
- Single coordinate space — layout module is source of truth for all positions

## Status: Feature Complete

**181 tests** (96 core + 79 wasm + 6 theme/minimap), all passing.

### Completed Features
- Phases 1-8: ECS, graph model, SVG rendering, interaction, groups, frames, reroutes, theme, minimap
- Phase 9: API ergonomics (add_node returns ports, connect_ports returns Result, topological sort, NodeTypeId, event callbacks)
- Inline port widgets (nodegraph-widgets crate with float_input)
- Right-click context menu with node/connection/frame actions
- Frame renaming (double-click) and color picker (context menu presets)
- Drag node onto wire to auto-insert + delete with auto-reconnect
- Full GraphEditor serialization with subgraph hierarchy
- Trivial-calculator example with graph evaluation and live JSON panel
- API review: internal modules hidden, re-exports at crate roots, rustdoc on key types

### Not Planned
- Light theme variant (deferred)
