# Getting started with dwind-nodes

A tour of the library by building a small "math nodes" app: Constant → Add → Display. By the end you'll know what every piece does and where to look next.

## What dwind-nodes is

A Rust + WASM framework for building node-graph editors in the browser. Four crates in the workspace:

| Crate | What it gives you |
|---|---|
| `nodegraph-core` | Graph data model: entities, nodes, ports, connections, groups, undo/redo, serialization. Pure data, no rendering. |
| `nodegraph-render` | SVG+dominator rendering: pan/zoom viewport, wires, node chrome, port hit-testing, search menu. Plus `GraphSignals` — the reactive hub everything else plugs into. |
| `nodegraph-widgets` | Compact inline port widgets: `float_input`, `int_input`, `bool_input`, `string_input`, `color_input`. |
| `nodegraph-runtime` | The thing that makes a graph *do* something. Per-type value storage, connection bridging, reactive signal plumbing, and the `NodeComputation` trait. |

Your app glues these together: declare your node types, implement their computations, and register everything at startup.

## The mental model

- A **port** has a direction (Input or Output), a `SocketType` (Float / Int / Bool / Color / Image / …), and a label. Ports belong to nodes.
- A **`ParamValue`** is a Rust type that flows across a socket. The library provides impls for the primitives; your app provides impls for its domain types.
- The **`ParamStore`** holds one `Mutable<T>` per editable port. Widgets read and write these.
- The **`Runtime`** holds, per registered type, a `TypedValueStore<T>` — output `Mutable<T>`s and source selectors for every port. It watches the graph's structure and sets everything up reactively.
- A **`NodeComputation`** describes how one node type computes. It's called once per node instance (on setup); it subscribes to its input signals and writes its output `Mutable`. From then on, everything is push-driven.
- **Connections** are handled by `Runtime::handle_connect`/`handle_disconnect` which either plug the upstream `Mutable` directly into the target's source selector (same type) or spawn a conversion bridge (cross-type, if registered).

That's the whole framework.

## Step 0: Workspace + crate setup

Your workspace `Cargo.toml`:

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
dominator = "0.5"
dwind = "0.7"
dwind-macros = "0.4"
futures-signals = "0.3"
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = "0.3"
console_error_panic_hook = "0.1"

nodegraph-core = { path = "path/to/dwind-nodes/crates/nodegraph-core" }
nodegraph-render = { path = "path/to/dwind-nodes/crates/nodegraph-render" }
nodegraph-widgets = { path = "path/to/dwind-nodes/crates/nodegraph-widgets" }
nodegraph-runtime = { path = "path/to/dwind-nodes/crates/nodegraph-runtime" }
```

Your app crate's `Cargo.toml`:

```toml
[package]
name = "math-nodes"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
nodegraph-core = { workspace = true }
nodegraph-render = { workspace = true }
nodegraph-widgets = { workspace = true }
nodegraph-runtime = { workspace = true }
dominator = { workspace = true }
dwind = { workspace = true }
dwind-macros = { workspace = true }
futures-signals = { workspace = true }
wasm-bindgen = { workspace = true }
wasm-bindgen-futures = { workspace = true }
console_error_panic_hook = { workspace = true }
web-sys = { workspace = true }
```

Your `index.html` (served by [trunk](https://trunkrs.dev/)):

```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <link data-trunk rel="rust" />
  <style>
    html, body { margin: 0; padding: 0; width: 100%; height: 100%;
                 overflow: hidden; background: #1a1a2e; }
  </style>
</head>
<body></body>
</html>
```

## Step 1: Declare your node types

Node types are data. Register them once with a `NodeTypeRegistry`; the library uses this for the Shift-A search menu and for typed-port construction.

```rust
// src/nodes.rs
use nodegraph_core::{NodeTypeDefinition, NodeTypeRegistry, PortDefinition, PortDirection, SocketType};

pub fn register_all(reg: &mut NodeTypeRegistry) {
    reg.register(NodeTypeDefinition {
        type_id: "const_float".into(),
        display_name: "Constant".into(),
        category: "Value".into(),
        input_ports: vec![],
        output_ports: vec![PortDefinition {
            direction: PortDirection::Output,
            socket_type: SocketType::Float,
            label: "Value".into(),
        }],
    });

    reg.register(NodeTypeDefinition {
        type_id: "add".into(),
        display_name: "Add".into(),
        category: "Math".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input,  socket_type: SocketType::Float, label: "A".into() },
            PortDefinition { direction: PortDirection::Input,  socket_type: SocketType::Float, label: "B".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Sum".into() },
        ],
    });

    reg.register(NodeTypeDefinition {
        type_id: "multiply".into(),
        display_name: "Multiply".into(),
        category: "Math".into(),
        input_ports: vec![
            PortDefinition { direction: PortDirection::Input,  socket_type: SocketType::Float, label: "A".into() },
            PortDefinition { direction: PortDirection::Input,  socket_type: SocketType::Float, label: "B".into() },
        ],
        output_ports: vec![
            PortDefinition { direction: PortDirection::Output, socket_type: SocketType::Float, label: "Product".into() },
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
```

## Step 2: Register value types

Your runtime needs to know which `ParamValue` types to allocate storage for. For a math app, `f64` is enough. The library provides builtin impls:

```rust
use nodegraph_runtime::prelude::*;

runtime.register_value_type::<f64>();
```

If you had a domain type of your own — say `Vec3` — you'd:

```rust
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_runtime::prelude::ParamValue;

#[derive(Clone, Default)]
pub struct Vec3 { pub x: f64, pub y: f64, pub z: f64 }

impl ParamValue for Vec3 {
    const SOCKET_TYPE: SocketType = SocketType::Vector;
}

runtime.register_value_type::<Vec3>();
```

Two rules:
1. `T: Clone + Default + 'static`. `Default` gives the runtime an initial output value before any computation fires.
2. **One Rust type per `SocketType`.** Registering two different `ParamValue` types under the same `SOCKET_TYPE` panics — the runtime can't disambiguate at lookup time.

## Step 3: Implement `NodeComputation`

One trait impl per node type. The `spawn` method is called once when a node is set up; it builds input signals, combines them, and writes to an output `Mutable`.

```rust
// src/computations.rs
use std::cell::Cell;
use std::rc::Rc;

use futures_signals::map_ref;
use futures_signals::signal::SignalExt;

use nodegraph_runtime::prelude::{NodeComputation, NodeCtx};

pub struct Add;
impl NodeComputation for Add {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let a = ctx.input_signal_or::<f64>("A", 0.0);
        let b = ctx.input_signal_or::<f64>("B", 0.0);
        let output = match ctx.output_mutable::<f64>("Sum") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let a = a,
                let b = b => { *a + *b }
            }
            .for_each(move |sum| {
                if alive.get() { output.set(sum); }
                async {}
            })
            .await;
        });
    }
}

pub struct Multiply;
impl NodeComputation for Multiply {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let a = ctx.input_signal_or::<f64>("A", 1.0);
        let b = ctx.input_signal_or::<f64>("B", 1.0);
        let output = match ctx.output_mutable::<f64>("Product") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let a = a,
                let b = b => { *a * *b }
            }
            .for_each(move |product| {
                if alive.get() { output.set(product); }
                async {}
            })
            .await;
        });
    }
}
```

Things to notice:

- **`input_signal_or(label, default)`**: gives you a `BoxSignal<T>` that follows an upstream connection when present, or the port's param `Mutable` otherwise. The `default` is used on first access — subsequent reads of the same port use whatever the widget has written since.
- **There's also `input_signal_default(label)`** which uses `T::default()`. Fine for "empty" types like `Texture` / `[u8; 4]` where zero is semantically a valid "no input" value; usually wrong for scalars (`0.0` / `0` / `false` is rarely what you meant). Prefer `input_signal_or` with an explicit default for primitives.
- **`alive`** is a `Rc<Cell<bool>>` the runtime flips to `false` when the node is torn down. Check it before every write so stale tasks don't keep mutating the output.
- `Display` is a sink with no output — no `NodeComputation` needed. The UI (step 5) handles showing it.

`Constant` is the standard pattern of "mirror the param `Mutable` onto the output `Mutable`." The library ships a generic implementation for this, so you don't need to write your own:

```rust
use nodegraph_runtime::prelude::ConstNode;

// in register_runtime (step 4):
runtime.computations().register(
    "const_float",
    Rc::new(ConstNode::<f64>::new(1.0)),
);
```

## Step 4: Wire the runtime at startup

```rust
// src/lib.rs
#[macro_use]
extern crate dwind_macros;

mod computations;
mod nodes;

use std::rc::Rc;

use dominator::html;
use dwind::prelude::*;
use nodegraph_render::{render_graph_editor, GraphSignals};
use nodegraph_runtime::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    dwind::stylesheet();

    let gs = GraphSignals::new();
    let params = ParamStore::new();

    {
        let mut reg = gs.registry.borrow_mut();
        nodes::register_all(&mut reg);
    }

    let runtime = Runtime::new(gs.clone(), params.clone());

    // 1. Value types we'll route.
    runtime.register_value_type::<f64>();

    // 2. Per-node-type computations.
    runtime.computations().register(
        "const_float",
        Rc::new(ConstNode::<f64>::new(1.0)) as Rc<dyn NodeComputation>,
    );
    runtime.computations().register("add", Rc::new(computations::Add));
    runtime.computations().register("multiply", Rc::new(computations::Multiply));
    // "display" has no computation — it's a sink.

    // 3. Port widgets (see step 5).
    gs.port_widget.borrow_mut().replace(make_port_widget(&params));

    // 4. Populate + start watching.
    runtime.initial_setup();
    runtime.spawn_reconciliation_watcher();

    // Migrate per-port Mutables across group/ungroup (if you use groups).
    {
        let params2 = params.clone();
        gs.set_on_group(move |_, _, port_map| params2.migrate_ports(&port_map));
    }
    {
        let params2 = params.clone();
        gs.set_on_ungroup(move |port_map| params2.migrate_ports(&port_map));
    }

    dominator::append_dom(
        &dominator::body(),
        html!("div", {
            .dwclass!("w-full h-full")
            .child(render_graph_editor(gs))
        }),
    );
}
```

Order matters:

1. `Runtime::new` — creates the orchestrator. Takes the `GraphSignals` + `ParamStore`.
2. `register_value_type::<T>()` for every `T` your graph uses. Do this **before** any `register_computation` call that touches `T`.
3. Register computations per `type_id`.
4. Install the port-widget callback (step 5).
5. `initial_setup` runs one reconciliation pass against the current graph state.
6. `spawn_reconciliation_watcher` subscribes to `node_list` / `connection_list` changes and reconciles on every edit. Without this, programmatic node/connection additions wouldn't wire up their computations.

## Step 5: Port widgets for editable inputs

Empty input ports should show an editable widget so users can set values without a separate panel. The `GraphSignals::port_widget` callback is fired for every port on every render; return `Some(Dom)` to inject a widget.

```rust
use nodegraph_core::{EntityId, PortDirection, SocketType};
use nodegraph_render::GraphSignals;
use nodegraph_runtime::prelude::ParamStore;
use nodegraph_widgets::float_input::{float_input, FloatInputProps, FloatValueWrapper};

#[allow(clippy::type_complexity)]
pub fn make_port_widget(
    params: &Rc<ParamStore>,
) -> Rc<dyn Fn(EntityId, EntityId, SocketType, PortDirection, &str, bool, &Rc<GraphSignals>) -> Option<dominator::Dom>> {
    let params = params.clone();
    Rc::new(move |_node_id, port_id, socket_type, port_dir, type_id, is_connected, _gs| {
        // Show widgets on: the output of const_float, or disconnected float inputs.
        let is_const_output = port_dir == PortDirection::Output && type_id == "const_float";
        let is_open_input = port_dir == PortDirection::Input && !is_connected;
        if !is_const_output && !is_open_input { return None; }

        match socket_type {
            SocketType::Float => {
                let mutable = params.get::<f64>(port_id, 0.0);
                Some(float_input(
                    FloatInputProps::new().value(Box::new(mutable) as Box<dyn FloatValueWrapper>),
                ))
            }
            _ => None,
        }
    })
}
```

Widgets from `nodegraph-widgets` accept a `Box<dyn *ValueWrapper>` — `ParamStore` hands you a `Mutable<T>` which already implements the wrapper trait, so the boilerplate stays small.

For the `Display` sink, you'd want to show the computed value. Use the `custom_node_body` callback and subscribe to the source selector via `runtime.input_signal_or::<f64>(input_port, 0.0)`:

```rust
use nodegraph_core::graph::port::PortDirection;

#[allow(clippy::type_complexity)]
pub fn make_custom_body(
    runtime: &Rc<Runtime>,
) -> Rc<dyn Fn(EntityId, &Rc<GraphSignals>) -> Option<dominator::Dom>> {
    let runtime = runtime.clone();
    Rc::new(move |node_id, gs| {
        let type_id = gs.with_graph(|g| {
            g.world
                .get::<nodegraph_core::graph::node::NodeTypeId>(node_id)
                .map(|t| t.0.clone())
                .unwrap_or_default()
        });
        if type_id != "display" { return None; }

        let input_port = gs.with_graph(|g| {
            g.node_ports(node_id)
                .iter()
                .find(|&&pid| g.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Input))
                .copied()
        })?;

        let sig = runtime.input_signal_or::<f64>(input_port, 0.0);
        Some(html!("div", {
            .dwclass!("flex items-center justify-center text-gray-100 py-2")
            .style("font-family", "-apple-system, BlinkMacSystemFont, sans-serif")
            .style("font-size", "18px")
            .text_signal(sig.map(|v| format!("{:.3}", v)))
        }))
    })
}

// then in main:
gs.custom_node_body.borrow_mut().replace(make_custom_body(&runtime));
```

## Step 6: Run it

```bash
cd math-nodes
trunk serve
```

Open the browser. You'll get an empty canvas. Shift-A opens the search menu — add a Constant, an Add, and a Display. Wire them up, edit the constants, watch Display react. Middle-mouse pans; scroll zooms.

## Going deeper

### Cross-type conversions

If you mix `SocketType::Int` and `SocketType::Float` inputs, the graph layer allows connecting them (because `SocketType::is_compatible_with` permits it). To actually convert values at runtime, register a conversion:

```rust
runtime.conversions().register::<i64, f64, _>(|i| i as f64);
runtime.conversions().register::<f64, i64, _>(|f| f as i64);
```

Each conversion spawns a bridge task on connect; `handle_disconnect` cancels it. Without a registration the graph still accepts the connection but the value silently drops.

### Const nodes for other types

Every Value-socket + widget combination you want should have a matching const-style node so values can fan out to many consumers:

```rust
runtime.computations().register("const_int",  Rc::new(ConstNode::<i64>::new(0)));
runtime.computations().register("const_bool", Rc::new(ConstNode::<bool>::new(false)));
```

Don't forget to also register the node types (`NodeTypeDefinition`) and update your `port_widget` callback to show the widget on the const output.

### Group nodes

`nodegraph-core` supports grouping selected nodes into a subgraph with IO ports. Runtime support is opt-in: implement a `Group` `NodeComputation` (texture-generator has one) and install it via `runtime.set_group_computation(Rc::new(YourGroup))`. The runtime invokes it for any node carrying a `SubgraphRoot` component whose `type_id` isn't in the normal computation registry.

### Custom value types

`ParamValue for MyType { const SOCKET_TYPE = SocketType::Custom(N); }` — the `Custom(u32)` variant gives you unique identity for app-specific sockets. Connection compatibility uses the custom tag, so a `Custom(1)` and `Custom(2)` won't wire together.

### Serialization

`nodegraph_core::SerializedGraphEditor` serializes the graph structure via `serde`. Your `ParamStore` state isn't included — wire your own save/load using `ParamStore::snapshot_type::<T>()` if you need it.

## Where to look next

- **`examples/texture-generator`** — the full-featured example. Every piece of the runtime is exercised: 14 node types, 6 value types, cross-type conversions, group computation, custom canvas bodies for previews.
- **`examples/trivial-calculator`** — a minimalist example that predates the runtime crate. Uses an imperative `evaluate()` loop instead of `NodeComputation`. Useful to compare against to see what the runtime buys you.
- **`crates/nodegraph-runtime/src/`** — the runtime crate source is small (~900 LOC). If you're curious how connection bridging or reconciliation works, read `runtime.rs` — it's the hot path.
- **Keyboard shortcuts** in any running app: press `?` to see the full list.

## Common gotchas

- **`port_widget` not firing on your custom port type?** Check that `socket_type` is one your match handles, and that `port_dir` / `is_connected` match your intended case. The callback runs on every connection-list change.
- **Edits to a widget don't reach the computation?** You're almost certainly using two different `Mutable`s for the same port. Make sure your widget dispatch and your `register_value_type` line up on the same `T` for each `SocketType`.
- **Panic about "SocketType already registered"?** You've impl'd `ParamValue` for two different Rust types that both claim the same `SocketType`. Pick one.
- **Dragging a value does nothing visually?** If you're hardcoding ports via `add_node_typed(...)` with `SocketType::Float` but your widget+computation expect `SocketType::Int` (or vice versa), the widget's `Mutable<f64>` and the computation's `Mutable<i64>` live in different stores. Keep port definitions and value types synchronized — ideally derive one from the other via the `NodeTypeRegistry`.
- **Scalar input defaulting to `0.0` when you expected something else?** You probably called `input_signal_default`; use `input_signal_or` with an explicit fallback.
