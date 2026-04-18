//! `NodeComputation` implementations — one per node type in `nodes.rs`.
//!
//! Each impl looks up its input signals via `NodeCtx::input_signal_or`,
//! combines them with `map_ref!`, and pushes the result into its output
//! `Mutable<T>`. The runtime handles port creation, connection wiring, and
//! lifecycle.

use std::cell::Cell;
use std::rc::Rc;

use futures_signals::map_ref;
use futures_signals::signal::SignalExt;

use nodegraph_runtime::prelude::{NodeComputation, NodeCtx};

use crate::eval;
use crate::params::{default_color, default_float, default_int};
use crate::texture::{Texture, TextureBuffer};

// ============================================================
// Generators
// ============================================================

pub struct Checker;
impl NodeComputation for Checker {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let ca = ctx.input_signal_or::<[u8; 4]>("Color A", default_color("checker", "Color A"));
        let cb = ctx.input_signal_or::<[u8; 4]>("Color B", default_color("checker", "Color B"));
        let size = ctx.input_signal_or::<i64>("Size", default_int("checker", "Size"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let ca = ca,
                let cb = cb,
                let size = size => { (*ca, *cb, *size) }
            }
            .for_each(move |(ca, cb, size)| {
                if alive.get() {
                    output.set(Texture::new(eval::eval_checker(ca, cb, size)));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Noise;
impl NodeComputation for Noise {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let scale = ctx.input_signal_or::<f64>("Scale", default_float("noise", "Scale"));
        let seed = ctx.input_signal_or::<i64>("Seed", default_int("noise", "Seed"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let scale = scale,
                let seed = seed => { (*scale, *seed) }
            }
            .for_each(move |(scale, seed)| {
                if alive.get() {
                    output.set(Texture::new(eval::eval_noise(scale, seed)));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Gradient;
impl NodeComputation for Gradient {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let ca = ctx.input_signal_or::<[u8; 4]>("Color A", default_color("gradient", "Color A"));
        let cb = ctx.input_signal_or::<[u8; 4]>("Color B", default_color("gradient", "Color B"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let ca = ca,
                let cb = cb => { (*ca, *cb) }
            }
            .for_each(move |(ca, cb)| {
                if alive.get() {
                    output.set(Texture::new(eval::eval_gradient(ca, cb)));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Brick;
impl NodeComputation for Brick {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let mortar = ctx.input_signal_or::<[u8; 4]>("Mortar", default_color("brick", "Mortar"));
        let brick = ctx.input_signal_or::<[u8; 4]>("Brick", default_color("brick", "Brick"));
        let rows = ctx.input_signal_or::<i64>("Rows", default_int("brick", "Rows"));
        let stagger = ctx.input_signal_or::<bool>("Stagger", true);
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let mortar = mortar,
                let brick = brick,
                let rows = rows,
                let stagger = stagger => { (*mortar, *brick, *rows, *stagger) }
            }
            .for_each(move |(mortar, brick, rows, stagger)| {
                if alive.get() {
                    output.set(Texture::new(eval::eval_brick(mortar, brick, rows, stagger)));
                }
                async {}
            })
            .await;
        });
    }
}

// ============================================================
// Filters
// ============================================================

pub struct Mix;
impl NodeComputation for Mix {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let a = ctx.input_signal_default::<Texture>("A");
        let b = ctx.input_signal_default::<Texture>("B");
        let factor = ctx.input_signal_or::<f64>("Factor", default_float("mix", "Factor"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let a = a,
                let b = b,
                let factor = factor => { (a.clone(), b.clone(), *factor) }
            }
            .for_each(move |(a, b, factor)| {
                if alive.get() {
                    let tex = eval::eval_mix(Some(a.0.clone()), Some(b.0.clone()), factor);
                    output.set(Texture::new(tex));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Blend;
impl NodeComputation for Blend {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let a = ctx.input_signal_default::<Texture>("A");
        let b = ctx.input_signal_default::<Texture>("B");
        let mask = ctx.input_signal_default::<Texture>("Mask");
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let a = a,
                let b = b,
                let mask = mask => { (a.clone(), b.clone(), mask.clone()) }
            }
            .for_each(move |(a, b, mask)| {
                if alive.get() {
                    let tex = eval::eval_blend(
                        Some(a.0.clone()),
                        Some(b.0.clone()),
                        Some(mask.0.clone()),
                    );
                    output.set(Texture::new(tex));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct BrightnessContrast;
impl NodeComputation for BrightnessContrast {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let tex = ctx.input_signal_default::<Texture>("Texture");
        let brightness = ctx.input_signal_or::<f64>(
            "Brightness",
            default_float("brightness_contrast", "Brightness"),
        );
        let contrast = ctx
            .input_signal_or::<f64>("Contrast", default_float("brightness_contrast", "Contrast"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let tex = tex,
                let brightness = brightness,
                let contrast = contrast => { (tex.clone(), *brightness, *contrast) }
            }
            .for_each(move |(tex, brightness, contrast)| {
                if alive.get() {
                    let result =
                        eval::eval_brightness_contrast(Some(tex.0.clone()), brightness, contrast);
                    output.set(Texture::new(result));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Threshold;
impl NodeComputation for Threshold {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let tex = ctx.input_signal_default::<Texture>("Texture");
        let level = ctx.input_signal_or::<f64>("Level", default_float("threshold", "Level"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let tex = tex,
                let level = level => { (tex.clone(), *level) }
            }
            .for_each(move |(tex, level)| {
                if alive.get() {
                    let result = eval::eval_threshold(Some(tex.0.clone()), level);
                    output.set(Texture::new(result));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Invert;
impl NodeComputation for Invert {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let tex = ctx.input_signal_default::<Texture>("Texture");
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            tex.for_each(move |tex| {
                if alive.get() {
                    let result = eval::eval_invert(Some(tex.0.clone()));
                    output.set(Texture::new(result));
                }
                async {}
            })
            .await;
        });
    }
}

pub struct Colorize;
impl NodeComputation for Colorize {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        let tex = ctx.input_signal_default::<Texture>("Texture");
        let tint = ctx.input_signal_or::<[u8; 4]>("Tint", default_color("colorize", "Tint"));
        let output = match ctx.output_mutable::<Texture>("Texture") {
            Some(o) => o,
            None => return,
        };
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let tex = tex,
                let tint = tint => { (tex.clone(), *tint) }
            }
            .for_each(move |(tex, tint)| {
                if alive.get() {
                    let result = eval::eval_colorize(Some(tex.0.clone()), tint);
                    output.set(Texture::new(result));
                }
                async {}
            })
            .await;
        });
    }
}

// ============================================================
// Group node
// ============================================================

/// Group nodes re-evaluate their subgraph imperatively on any input
/// change. We fan all input signals into a single version counter,
/// then recompute via `eval::evaluate` and write to group output ports.
///
/// This preserves the existing imperative-eval path for grouped subgraphs
/// — a follow-up could make groups fully signal-driven by having each
/// inner node's `NodeComputation::spawn` reach across the group boundary,
/// but that's a bigger redesign.
pub struct Group;
impl NodeComputation for Group {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>) {
        use futures_signals::signal::Mutable;
        use nodegraph_core::graph::group::SubgraphRoot;
        use nodegraph_core::graph::port::PortDirection;
        use nodegraph_core::types::socket_type::SocketType;

        let node_id = ctx.node_id();
        let gs = ctx.runtime().gs().clone();
        let rt = ctx.runtime().clone();
        let params = rt.params().clone();

        // Collect input signals so we fire on any upstream change.
        let mut input_sigs: Vec<nodegraph_runtime::prelude::BoxSignal<()>> = Vec::new();
        for &(pid, dir, stype, ref _label) in ctx.ports() {
            if dir != PortDirection::Input {
                continue;
            }
            match stype {
                SocketType::Image => input_sigs.push(Box::pin(
                    rt.input_signal_default::<Texture>(pid).map(|_| ()),
                )),
                SocketType::Color => input_sigs.push(Box::pin(
                    rt.input_signal_default::<[u8; 4]>(pid).map(|_| ()),
                )),
                SocketType::Float => {
                    input_sigs.push(Box::pin(rt.input_signal_default::<f64>(pid).map(|_| ())))
                }
                SocketType::Int => {
                    input_sigs.push(Box::pin(rt.input_signal_default::<i64>(pid).map(|_| ())))
                }
                SocketType::Bool => {
                    input_sigs.push(Box::pin(rt.input_signal_default::<bool>(pid).map(|_| ())))
                }
                _ => {}
            }
        }

        let version = Mutable::new(0u64);
        for sig in input_sigs {
            let version = version.clone();
            wasm_bindgen_futures::spawn_local(async move {
                sig.for_each(move |_| {
                    version.set(version.get().wrapping_add(1));
                    async {}
                })
                .await;
            });
        }

        // Also watch internal subgraph param signals so edits inside the
        // group trigger recomputation of group outputs.
        let subgraph_id = gs.with_graph(|g| g.world.get::<SubgraphRoot>(node_id).map(|s| s.0));
        if let Some(sub_id) = subgraph_id {
            let editor = gs.editor.borrow();
            if let Some(sub) = editor.graph(sub_id) {
                for (nid, _) in sub.world.query::<nodegraph_core::graph::node::NodeTypeId>() {
                    for &pid in sub.node_ports(nid) {
                        if sub.world.get::<PortDirection>(pid).copied()
                            != Some(PortDirection::Input)
                        {
                            continue;
                        }
                        // Seed only — we don't need to keep references,
                        // just subscribe once to each param signal.
                        let stype = sub
                            .world
                            .get::<nodegraph_core::graph::port::PortSocketType>(pid)
                            .map(|s| s.0);
                        let version = version.clone();
                        match stype {
                            Some(SocketType::Float) => {
                                let m = params.get::<f64>(pid, 0.0);
                                wasm_bindgen_futures::spawn_local(async move {
                                    m.signal_cloned()
                                        .for_each(move |_| {
                                            version.set(version.get().wrapping_add(1));
                                            async {}
                                        })
                                        .await;
                                });
                            }
                            Some(SocketType::Int) => {
                                let m = params.get::<i64>(pid, 0);
                                wasm_bindgen_futures::spawn_local(async move {
                                    m.signal_cloned()
                                        .for_each(move |_| {
                                            version.set(version.get().wrapping_add(1));
                                            async {}
                                        })
                                        .await;
                                });
                            }
                            Some(SocketType::Bool) => {
                                let m = params.get::<bool>(pid, false);
                                wasm_bindgen_futures::spawn_local(async move {
                                    m.signal_cloned()
                                        .for_each(move |_| {
                                            version.set(version.get().wrapping_add(1));
                                            async {}
                                        })
                                        .await;
                                });
                            }
                            Some(SocketType::Color) => {
                                let m = params.get::<[u8; 4]>(pid, [0, 0, 0, 255]);
                                wasm_bindgen_futures::spawn_local(async move {
                                    m.signal_cloned()
                                        .for_each(move |_| {
                                            version.set(version.get().wrapping_add(1));
                                            async {}
                                        })
                                        .await;
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Collect output ports for the group node.
        let tex_out_ports: Vec<nodegraph_core::EntityId> = ctx
            .ports()
            .iter()
            .filter(|(_, d, s, _)| *d == PortDirection::Output && *s == SocketType::Image)
            .map(|(id, _, _, _)| *id)
            .collect();
        let color_out_ports: Vec<nodegraph_core::EntityId> = ctx
            .ports()
            .iter()
            .filter(|(_, d, s, _)| *d == PortDirection::Output && *s == SocketType::Color)
            .map(|(id, _, _, _)| *id)
            .collect();

        let tex_outputs: std::collections::HashMap<_, _> = tex_out_ports
            .iter()
            .filter_map(|&pid| rt.get_output::<Texture>(pid).map(|m| (pid, m)))
            .collect();
        let color_outputs: std::collections::HashMap<_, _> = color_out_ports
            .iter()
            .filter_map(|&pid| rt.get_output::<[u8; 4]>(pid).map(|m| (pid, m)))
            .collect();

        wasm_bindgen_futures::spawn_local(async move {
            version
                .signal()
                .for_each(move |_| {
                    if alive.get() {
                        let snap = crate::eval::ParamSnapshot {
                            floats: params.snapshot_type::<f64>(),
                            ints: params.snapshot_type::<i64>(),
                            bools: params.snapshot_type::<bool>(),
                            strings: params.snapshot_type::<String>(),
                            colors: params.snapshot_type::<[u8; 4]>(),
                        };
                        let editor = gs.editor.borrow();
                        let result = crate::eval::evaluate(&editor, &snap);
                        drop(editor);

                        for (pid, mutable) in &tex_outputs {
                            if let Some(t) = result.textures.get(pid) {
                                mutable.set(Texture(t.clone()));
                            } else {
                                mutable.set(Texture(Rc::new(TextureBuffer::new())));
                            }
                        }
                        for (pid, mutable) in &color_outputs {
                            if let Some(c) = result.colors.get(pid) {
                                mutable.set(*c);
                            }
                        }
                    }
                    async {}
                })
                .await;
        });
    }
}

// ============================================================
// Output / sink nodes
// ============================================================
//
// Sink nodes (preview, tiled_preview, iso_preview, block_preview) don't
// produce outputs — their only job is to render their input into a canvas,
// which is handled by `preview::make_custom_body`. No `NodeComputation`
// is registered for them; they fall through the runtime's dispatch silently.
