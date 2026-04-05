# dwind-nodes — Implementation Plan

Blender-style node graph UI library built on dwind/dominator/futures-signals (Rust/WASM).

## Architecture

- **nodegraph-core**: Custom ECS store, graph model, interaction state machine, layout, serialization, search registry, topological sort
- **nodegraph-render**: SVG rendering via dominator, reactive signal bridge (GraphSignals), event handling, search menu UI, minimap
- **nodegraph-widgets**: Compact inline input components (`float_input`, etc.) using `#[component]` macro
- **nodegraph-demo**: Demo app with Blender-style node types
- **examples/trivial-calculator**: Minimal example with graph evaluation and live JSON panel

### Core Principles
- Pure FRP: DOM as computation over mutable state, no imperative sync
- SVG rendering for graph structure, foreignObject for HTML node content
- Snapshot-based undo (full GraphEditor clone, not command pattern)
- Single coordinate space — layout module is source of truth for all positions

## Completed

### Phase 1-7 — Core Graph System
- Custom ECS World with generational indices, CloneableStore for snapshot undo
- SVG rendering, pan/zoom, drag-to-connect, box select, cut links
- Node type registry with search menu, noodle-drop-to-add
- Node groups with subgraph navigation, individual IO nodes per connection
- Frames with reactive bounds, reroute diamond rendering
- Snapshot-based undo/redo

### Phase 8 — Theme System & Minimap
- Theme struct centralizing ~40 color/opacity values with `Theme::dark()`
- Minimap with node rects, connection lines, viewport rect, click-to-pan
- graph_bounds tracking, viewport_size

### Phase 9 — API Stabilization (partial)
- `add_node()` returns `(EntityId, Vec<EntityId>)` — port IDs immediately available
- `add_node_typed()` sets `NodeTypeId` component for type dispatch
- `connect_ports()` returns `Result<EntityId, ConnectionError>`
- `spawn_from_registry()` stores `NodeTypeId` on nodes
- `PortWidgetFn` receives `port_direction` and `node_type_id` arguments
- `NodeGraph::topological_sort()` with cycle detection (Kahn's algorithm)
- `NodeGraph::eval_order()` convenience method
- `custom_node_body` and `port_widget` callbacks for user-defined rendering
- `nodegraph-widgets` crate with `float_input` component
- Trivial-calculator example with graph evaluation, inline widgets, live JSON

### Polish
- Delta-based full_sync (sync_entity_list with change detection)
- Graph-switch detection (clears stale entity IDs on subgraph navigation)
- Collapsed nodes explicitly hide port circles
- Selection state: select_single uses sync_selection
- Reroute layout in compute_node_layout (diamond-edge port positions)
- Frame selection/deletion, serialization roundtrip
- Search menu reactively filters by pending connection type

## Current State

- **180 tests** (95 core + 79 wasm + 6 theme/minimap), all passing

## Remaining Work

### Phase 9 — API Stabilization (remaining)
- [ ] Public API review: hide internal types, clean up pub visibility
- [ ] Event callbacks: on_connect, on_disconnect, on_selection_changed, on_node_moved
- [ ] Documentation: rustdoc on public types, usage examples
- [ ] Serialize/deserialize full GraphEditor (including subgraph hierarchy metadata)
- [ ] Light theme variant

### Nice-to-have
- [ ] Frame renaming UI
- [ ] Frame color picker UI
- [ ] Right-click context menu
- [ ] Drag reroute onto wire to insert
