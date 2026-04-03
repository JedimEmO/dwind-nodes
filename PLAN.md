# dwind-nodes — Implementation Plan

Blender-style node graph UI library built on dwind/dominator/futures-signals (Rust/WASM).

## Architecture

- **nodegraph-core**: Custom ECS store, graph model, interaction state machine, layout, serialization, search registry
- **nodegraph-render**: SVG rendering via dominator, reactive signal bridge (GraphSignals), event handling, search menu UI
- **nodegraph-demo**: Demo app with Blender-style node types

### Core Principles
- Pure FRP: DOM as computation over mutable state, no imperative sync
- SVG rendering for graph structure, foreignObject for HTML node content
- Snapshot-based undo (full GraphEditor clone, not command pattern)
- Single coordinate space — layout module is source of truth for all positions

## Completed

### Phase 1 — ECS Store & Graph Schema
- Custom ECS World with generational indices, typed component storage
- CloneableStore trait for type-erased World::clone (enables snapshot undo)
- NodeGraph facade: add_node, add_port, connect, disconnect, remove_node

### Phase 2 — Viewport, Layout & Interaction
- Viewport with pan/zoom transforms
- Layout constants: HEADER_HEIGHT, PORT_HEIGHT, PORT_RADIUS, NODE_MIN_WIDTH, REROUTE_SIZE
- LayoutCache: precomputed node layouts, connection paths, frame rects
- InteractionController state machine: Idle, Panning, DraggingNodes, ConnectingPort, BoxSelecting, CuttingLinks
- Hit testing: ports > nodes > connections > frames

### Phase 3 — Undo/Redo
- ~~Command pattern~~ replaced by snapshot-based UndoHistory
- UndoHistory: save() clones entire GraphEditor, undo/redo swap snapshots
- Copy/paste via SerializedGraph (preserves IsReroute markers)

### Phase 4 — Reactive Rendering
- GraphSignals: central reactive bridge between ECS and DOM
- Per-node Mutable for positions, headers; per-frame Mutable for bounds
- SVG <g> per node, <circle> ports, bezier <path> connections
- foreignObject for HTML content (header, port labels)

### Phase 5 — SVG Rewrite & Full Interaction Suite
- Pure SVG rendering (replaced HTML div approach)
- Drag-to-connect with preview wire
- Box selection, cut links (Ctrl+RMB polyline)
- Port highlighting during drag (scale, glow, direction+type filtering)
- Gradient stroke for type-conversion connections

### Phase 6 — Node Type Registry & Search Menu
- NodeTypeRegistry with search/filter/compatible port matching
- Search menu (HTML overlay, not SVG — event propagation issues)
- Text filtering, arrow navigation, Enter confirm, Escape close
- Noodle-drop-to-add: filtered to compatible types, auto-connects after spawn
- SocketType::Any for reroute pass-through compatibility
- Click-outside-to-close via el.closest("[data-search-menu]")

### Phase 7 — Node Groups & Subgraph Navigation
- GraphEditor: HashMap<EntityId, NodeGraph> for multiple subgraphs
- group_nodes(): moves nodes to subgraph, creates Group IO nodes, reconnects externals
- ungroup(): restores nodes + connections via IO port mapping
- Double-click group node to enter, breadcrumb navigation
- adapt_group_io_port(): type adaptation when connecting to IO ports
- O(1) caches: subgraph_parents, io_port_mapping

### Frames & Reroutes
- Frames: visual grouping with reactive bounds tracking member positions
- Frame drag moves all member nodes, frame selection + Delete key
- Reroute nodes: diamond polygon rendering, SocketType::Any pass-through
- port_offset handles reroute diamond-edge coordinates

### Polish & Fixes
- Frame serialization (label, color, members) with roundtrip test
- Signal map cleanup (node_positions, node_headers, frame_bounds) on subgraph navigation
- Search menu opens at cursor position (not viewport center)
- Search menu reactively filters by pending connection type (map_ref! over both signals)
- Deduplicated FrameDeselect logic, broadcast() for frame selection signal

## Current State

- **165 tests** (94 core + 71 wasm), all passing
- **93% line coverage** on nodegraph-core

## Remaining Work

### Phase 8 — Minimap & Theme System
- [ ] Minimap: scaled-down overview of the graph, viewport rectangle indicator, click-to-navigate
- [ ] Theme system: configurable colors, fonts, sizes via a Theme struct
- [ ] Dark/light theme support
- [ ] Node color customization (per-type via registry)

### Phase 9 — API Stabilization & Docs
- [ ] Public API review: hide internal types, clean up pub visibility
- [ ] Builder pattern for node construction
- [ ] Event callbacks: on_connect, on_disconnect, on_selection_changed, on_node_moved
- [ ] Documentation: rustdoc on public types, usage examples
- [ ] Serialize/deserialize full GraphEditor (including subgraphs, not just single NodeGraph)

### Known Rough Edges
- [ ] full_sync DOM thrashing: rebuilds entire node/connection/frame lists on every undo/redo
- [ ] Selection state duplication: InteractionController.selection vs GraphSignals.selection
- [ ] Collapse visual: collapsed nodes don't hide port circles in SVG
- [ ] Frame renaming: no UI to rename a frame after creation
- [ ] Frame color picker: no UI to change frame color
- [ ] node_positions/node_headers HashMaps leak on subgraph navigation (retain added for full_sync, but sync_all_positions still iterates stale entries)
- [ ] Reroute type narrowing: Any passes everything through without type propagation (by design, but Blender narrows)

### Testing Gaps
- [ ] Frame title rendering (foreignObject) — no DOM test
- [ ] Reroute diamond rendering — no DOM test verifying SVG polygon
- [ ] port_offset for reroutes — no test verifying exact coordinates
- [ ] Frame redo cycle — only undo tested
- [ ] Multiple reroutes in series (A→R1→R2→B)
- [ ] Serialization roundtrip for full GraphEditor with subgraphs
