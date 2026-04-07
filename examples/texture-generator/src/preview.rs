use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use dominator::html;
use wasm_bindgen::JsCast;
use wasm_bindgen::Clamped;

use nodegraph_core::EntityId;
use nodegraph_core::graph::node::NodeTypeId;
use nodegraph_core::graph::port::PortDirection;
use nodegraph_render::GraphSignals;

use crate::params::ParamStore;
use crate::texture::{TextureBuffer, TEX_SIZE};

/// Canvas entry with its node type for choosing the right render mode.
pub struct CanvasEntry {
    canvas: web_sys::HtmlCanvasElement,
    node_type: String,
}

pub type CanvasRegistry = Rc<RefCell<HashMap<EntityId, CanvasEntry>>>;

pub fn new_canvas_registry() -> CanvasRegistry {
    Rc::new(RefCell::new(HashMap::new()))
}

/// Canvas dimensions for each node type.
fn canvas_dims(type_id: &str) -> (u32, u32, &'static str) {
    // (canvas_width, canvas_height, css_size)
    match type_id {
        "tiled_preview" => (TEX_SIZE as u32 * 4, TEX_SIZE as u32 * 4, "140"),
        "iso_preview" => (96, 96, "140"),
        "preview" => (TEX_SIZE as u32, TEX_SIZE as u32, "140"),
        _ => (TEX_SIZE as u32, TEX_SIZE as u32, "80"), // inline previews
    }
}

/// Build the custom_node_body callback that shows texture previews inside nodes.
pub fn make_custom_body(
    canvases: &CanvasRegistry,
    gs: &Rc<GraphSignals>,
    params: &Rc<ParamStore>,
) -> Rc<dyn Fn(EntityId, &Rc<GraphSignals>) -> Option<dominator::Dom>> {
    let canvases = canvases.clone();
    let gs_eval = gs.clone();
    let params = params.clone();
    // Coalesce multiple after_inserted callbacks into a single deferred evaluation
    let eval_scheduled = Rc::new(Cell::new(false));
    Rc::new(move |node_id, gs| {
        let type_id = gs.with_graph(|g| {
            g.world.get::<NodeTypeId>(node_id).map(|t| t.0.clone()).unwrap_or_default()
        });

        let has_image_output = matches!(type_id.as_str(),
            "checker" | "noise" | "gradient" | "brick" |
            "mix" | "brightness_contrast" | "threshold" | "invert" | "colorize"
        );
        let is_output = matches!(type_id.as_str(), "preview" | "tiled_preview" | "iso_preview");

        if !has_image_output && !is_output {
            return None;
        }

        let (cw, ch, css_size) = canvas_dims(&type_id);
        let canvases = canvases.clone();
        let gs_eval = gs_eval.clone();
        let params = params.clone();
        let type_id_owned = type_id.clone();
        let eval_scheduled = eval_scheduled.clone();

        Some(html!("div", {
            .style("display", "flex")
            .style("justify-content", "center")
            .style("padding", "4px 0")
            .style("pointer-events", "none")
            .child(html!("canvas" => web_sys::HtmlCanvasElement, {
                .attr("width", &cw.to_string())
                .attr("height", &ch.to_string())
                .style("width", &format!("{}px", css_size))
                .style("height", &format!("{}px", css_size))
                .style("image-rendering", "pixelated")
                .style("border", "1px solid #333")
                .style("border-radius", "2px")
                .style("background", "#000")
                .after_inserted(move |el| {
                    let canvas: web_sys::HtmlCanvasElement = el.unchecked_into();
                    canvases.borrow_mut().insert(node_id, CanvasEntry {
                        canvas,
                        node_type: type_id_owned,
                    });
                    // Defer evaluation so multiple canvas insertions coalesce into one
                    if !eval_scheduled.get() {
                        eval_scheduled.set(true);
                        let gs_eval = gs_eval.clone();
                        let params = params.clone();
                        let canvases = canvases.clone();
                        let eval_scheduled = eval_scheduled.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            eval_scheduled.set(false);
                            let result = crate::eval::evaluate(&gs_eval, &params);
                            update_previews(&canvases, &result.textures, &gs_eval);
                        });
                    }
                })
            }))
        }))
    })
}

/// Update all registered canvases with texture data from evaluation.
/// Textures are keyed by port EntityId, so we find the node's relevant port.
pub fn update_previews(
    canvases: &CanvasRegistry,
    textures: &HashMap<EntityId, Rc<TextureBuffer>>,
    gs: &Rc<GraphSignals>,
) {
    let canvases = canvases.borrow();
    for (&node_id, entry) in canvases.iter() {
        let tex = find_node_texture(node_id, &entry.node_type, textures, gs);
        let tex = match tex {
            Some(t) => t,
            None => continue,
        };

        match entry.node_type.as_str() {
            "tiled_preview" => render_tiled(&entry.canvas, &tex),
            "iso_preview" => render_isometric(&entry.canvas, &tex),
            _ => render_direct(&entry.canvas, &tex),
        }
    }
}

/// Find the texture for a node by looking up its ports in the eval result.
fn find_node_texture(
    node_id: EntityId,
    node_type: &str,
    textures: &HashMap<EntityId, Rc<TextureBuffer>>,
    gs: &Rc<GraphSignals>,
) -> Option<Rc<TextureBuffer>> {
    // For sink nodes (preview/tiled/iso), the texture is stored under the input port.
    // For all other nodes, it's stored under the output port.
    let is_sink = matches!(node_type, "preview" | "tiled_preview" | "iso_preview");
    let target_dir = if is_sink { PortDirection::Input } else { PortDirection::Output };

    gs.with_graph(|g| {
        for &pid in g.node_ports(node_id) {
            if g.world.get::<PortDirection>(pid).copied() == Some(target_dir) {
                if let Some(t) = textures.get(&pid) {
                    return Some(t.clone());
                }
            }
        }
        None
    })
}

/// Standard 1:1 putImageData.
fn render_direct(canvas: &web_sys::HtmlCanvasElement, tex: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) { Some(c) => c, None => return };
    let pixels = tex.as_u8_slice();
    let image_data = match web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&pixels), TEX_SIZE as u32, TEX_SIZE as u32,
    ) { Ok(d) => d, Err(_) => return };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

/// 4x4 tiled repeat of the texture.
fn render_tiled(canvas: &web_sys::HtmlCanvasElement, tex: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) { Some(c) => c, None => return };
    let tiles = 4usize;
    let size = TEX_SIZE * tiles;
    let mut pixels = vec![0u8; size * size * 4];
    for ty in 0..size {
        for tx in 0..size {
            let src = tex.data[(ty % TEX_SIZE) * TEX_SIZE + (tx % TEX_SIZE)];
            let dst = (ty * size + tx) * 4;
            pixels[dst..dst + 4].copy_from_slice(&src);
        }
    }
    let image_data = match web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&pixels), size as u32, size as u32,
    ) { Ok(d) => d, Err(_) => return };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

/// Isometric cube with top, left, and right faces.
///
/// Geometry (96x96 canvas):
///   Top vertex A = (48, 0)
///   Right vertex B = (96, 24)
///   Front vertex G = (48, 48)
///   Left vertex F = (0, 24)
///   Bottom-left E = (0, 72)
///   Bottom-right C = (96, 72)
///   Bottom vertex D = (48, 96)
///
/// Top face: A-B-G-F    (full brightness)
/// Left face: F-G-D-E   (0.7 brightness)
/// Right face: G-B-C-D  (0.5 brightness)
fn render_isometric(canvas: &web_sys::HtmlCanvasElement, tex: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) { Some(c) => c, None => return };
    let w = 96usize;
    let h = 96usize;
    let mut pixels = vec![0u8; w * h * 4];

    for py in 0..h {
        for px in 0..w {
            let (fpx, fpy) = (px as f64, py as f64);

            // Try top face: A(48,0) B(96,24) G(48,48) F(0,24)
            // P = A + u*(B-A) + v*(F-A) = (48+48u-48v, 24u+24v)
            // u = (px - 48 + 2*py) / 96, v = py/24 - u
            let u = (fpx - 48.0 + 2.0 * fpy) / 96.0;
            let v = fpy / 24.0 - u;
            if u >= 0.0 && u <= 1.0 && v >= 0.0 && v <= 1.0 {
                let c = sample_tex(tex, u, v);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Try left face: F(0,24) G(48,48) D(48,96) E(0,72)
            // P = F + u*(G-F) + v*(E-F) = (48u, 24+24u+48v)
            // u = px/48, v = (py-24-24u)/48
            let u = fpx / 48.0;
            let v = (fpy - 24.0 - 24.0 * u) / 48.0;
            if u >= 0.0 && u < 1.0 && v >= 0.0 && v <= 1.0 {
                let c = sample_tex_shaded(tex, u, v, 0.7);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Try right face: G(48,48) B(96,24) C(96,72) D(48,96)
            // P = G + u*(B-G) + v*(D-G) = (48+48u, 48-24u+48v)
            // u = (px-48)/48, v = (py-48+24u)/48
            let u = (fpx - 48.0) / 48.0;
            let v = (fpy - 48.0 + 24.0 * u) / 48.0;
            if u >= 0.0 && u <= 1.0 && v >= 0.0 && v <= 1.0 {
                let c = sample_tex_shaded(tex, u, v, 0.5);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Background: transparent (already 0)
        }
    }

    let image_data = match web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&pixels), w as u32, h as u32,
    ) { Ok(d) => d, Err(_) => return };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

fn sample_tex(tex: &TextureBuffer, u: f64, v: f64) -> [u8; 4] {
    let tx = ((u * TEX_SIZE as f64) as usize).min(TEX_SIZE - 1);
    let ty = ((v * TEX_SIZE as f64) as usize).min(TEX_SIZE - 1);
    tex.data[ty * TEX_SIZE + tx]
}

fn sample_tex_shaded(tex: &TextureBuffer, u: f64, v: f64, shade: f64) -> [u8; 4] {
    let [r, g, b, a] = sample_tex(tex, u, v);
    [
        (r as f64 * shade) as u8,
        (g as f64 * shade) as u8,
        (b as f64 * shade) as u8,
        a,
    ]
}

fn set_pixel(buf: &mut [u8], stride: usize, x: usize, y: usize, c: [u8; 4]) {
    let i = (y * stride + x) * 4;
    buf[i..i + 4].copy_from_slice(&c);
}

fn get_2d_ctx(canvas: &web_sys::HtmlCanvasElement) -> Option<web_sys::CanvasRenderingContext2d> {
    canvas.get_context("2d").ok()?
        .map(|ctx| ctx.unchecked_into())
}
