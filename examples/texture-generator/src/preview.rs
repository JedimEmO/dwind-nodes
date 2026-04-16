use std::rc::Rc;

use dominator::html;
use futures_signals::signal::SignalExt;
use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;

use nodegraph_core::graph::node::NodeTypeId;
use nodegraph_core::EntityId;
use nodegraph_render::GraphSignals;

use crate::reactive_eval::ReactiveEval;
use crate::texture::{TextureBuffer, TEX_SIZE};

/// Canvas dimensions for each node type.
fn canvas_dims(type_id: &str) -> (u32, u32, &'static str) {
    match type_id {
        "tiled_preview" => (TEX_SIZE as u32 * 4, TEX_SIZE as u32 * 4, "140"),
        "iso_preview" | "block_preview" => (96, 96, "140"),
        "preview" => (TEX_SIZE as u32, TEX_SIZE as u32, "140"),
        _ => (TEX_SIZE as u32, TEX_SIZE as u32, "80"),
    }
}

/// Build the custom_node_body callback that shows texture previews inside nodes.
/// Each canvas reactively watches its node's texture signal.
#[allow(clippy::type_complexity)]
pub fn make_custom_body(
    reval: &Rc<ReactiveEval>,
) -> Rc<dyn Fn(EntityId, &Rc<GraphSignals>) -> Option<dominator::Dom>> {
    let reval = reval.clone();
    Rc::new(move |node_id, gs| {
        let type_id = gs.with_graph(|g| {
            g.world
                .get::<NodeTypeId>(node_id)
                .map(|t| t.0.clone())
                .unwrap_or_default()
        });

        let has_image_output = matches!(
            type_id.as_str(),
            "checker"
                | "noise"
                | "gradient"
                | "brick"
                | "mix"
                | "blend"
                | "brightness_contrast"
                | "threshold"
                | "invert"
                | "colorize"
        );
        let is_output = matches!(
            type_id.as_str(),
            "preview" | "tiled_preview" | "iso_preview" | "block_preview"
        );

        if !has_image_output && !is_output {
            return None;
        }

        let (cw, ch, css_size) = canvas_dims(&type_id);
        let reval = reval.clone();
        let type_id_for_signal = type_id.clone();
        let type_id_for_render = type_id.clone();

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
                    let render_type = type_id_for_render;

                    if render_type == "block_preview" {
                        let sig = reval.block_preview_signal(node_id);
                        wasm_bindgen_futures::spawn_local(async move {
                            sig.for_each(move |(top, side)| {
                                render_block(&canvas, &top, &side);
                                async {}
                            }).await;
                        });
                    } else {
                        let sig = reval.texture_signal_for_node(node_id, &type_id_for_signal);
                        wasm_bindgen_futures::spawn_local(async move {
                            sig.for_each(move |tex| {
                                match render_type.as_str() {
                                    "tiled_preview" => render_tiled(&canvas, &tex),
                                    "iso_preview" => render_isometric(&canvas, &tex),
                                    _ => render_direct(&canvas, &tex),
                                }
                                async {}
                            }).await;
                        });
                    }
                })
            }))
        }))
    })
}

// ============================================================
// Rendering functions
// ============================================================

/// Standard 1:1 putImageData.
fn render_direct(canvas: &web_sys::HtmlCanvasElement, tex: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) {
        Some(c) => c,
        None => return,
    };
    let pixels = tex.as_u8_slice();
    let image_data = match web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&pixels),
        TEX_SIZE as u32,
        TEX_SIZE as u32,
    ) {
        Ok(d) => d,
        Err(_) => return,
    };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

/// 4x4 tiled repeat of the texture.
fn render_tiled(canvas: &web_sys::HtmlCanvasElement, tex: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) {
        Some(c) => c,
        None => return,
    };
    let pixels = tiled_pixels(tex);
    let size = TEX_SIZE * 4;
    let image_data = match web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&pixels),
        size as u32,
        size as u32,
    ) {
        Ok(d) => d,
        Err(_) => return,
    };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

/// Isometric cube with top, left, and right faces.
fn render_isometric(canvas: &web_sys::HtmlCanvasElement, tex: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) {
        Some(c) => c,
        None => return,
    };
    let pixels = isometric_pixels(tex);
    let image_data =
        match web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), 96, 96) {
            Ok(d) => d,
            Err(_) => return,
        };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

/// Isometric cube with separate textures for the top face vs the side faces.
fn render_block(canvas: &web_sys::HtmlCanvasElement, top: &TextureBuffer, side: &TextureBuffer) {
    let ctx = match get_2d_ctx(canvas) {
        Some(c) => c,
        None => return,
    };
    let pixels = block_preview_pixels(top, side);
    let image_data =
        match web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), 96, 96) {
            Ok(d) => d,
            Err(_) => return,
        };
    let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
}

// ============================================================
// Pure pixel-computation helpers (testable without web_sys)
// ============================================================

pub(crate) fn tiled_pixels(tex: &TextureBuffer) -> Vec<u8> {
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
    pixels
}

/// Two-texture variant of `isometric_pixels`: `top` is used on the top face at
/// full brightness; `side` is used on both vertical faces (left at 0.7×, right
/// at 0.5× shade). Face geometry matches `isometric_pixels`.
pub(crate) fn block_preview_pixels(top: &TextureBuffer, side: &TextureBuffer) -> Vec<u8> {
    let w = 96usize;
    let h = 96usize;
    let mut pixels = vec![0u8; w * h * 4];

    for py in 0..h {
        for px in 0..w {
            let (fpx, fpy) = (px as f64, py as f64);

            // Top face
            let u = (fpx - 48.0 + 2.0 * fpy) / 96.0;
            let v = fpy / 24.0 - u;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let c = sample_tex(top, u, v);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Left face (side texture, darker shade)
            let u = fpx / 48.0;
            let v = (fpy - 24.0 - 24.0 * u) / 48.0;
            if (0.0..1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let c = sample_tex_shaded(side, u, v, 0.7);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Right face (side texture, darkest shade)
            let u = (fpx - 48.0) / 48.0;
            let v = (fpy - 48.0 + 24.0 * u) / 48.0;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let c = sample_tex_shaded(side, u, v, 0.5);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }
        }
    }

    pixels
}

pub(crate) fn isometric_pixels(tex: &TextureBuffer) -> Vec<u8> {
    let w = 96usize;
    let h = 96usize;
    let mut pixels = vec![0u8; w * h * 4];

    for py in 0..h {
        for px in 0..w {
            let (fpx, fpy) = (px as f64, py as f64);

            // Top face: A(48,0) B(96,24) G(48,48) F(0,24)
            let u = (fpx - 48.0 + 2.0 * fpy) / 96.0;
            let v = fpy / 24.0 - u;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let c = sample_tex(tex, u, v);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Left face: F(0,24) G(48,48) D(48,96) E(0,72)
            let u = fpx / 48.0;
            let v = (fpy - 24.0 - 24.0 * u) / 48.0;
            if (0.0..1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let c = sample_tex_shaded(tex, u, v, 0.7);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }

            // Right face: G(48,48) B(96,24) C(96,72) D(48,96)
            let u = (fpx - 48.0) / 48.0;
            let v = (fpy - 48.0 + 24.0 * u) / 48.0;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let c = sample_tex_shaded(tex, u, v, 0.5);
                set_pixel(&mut pixels, w, px, py, c);
                continue;
            }
        }
    }

    pixels
}

pub(crate) fn sample_tex(tex: &TextureBuffer, u: f64, v: f64) -> [u8; 4] {
    let tx = ((u * TEX_SIZE as f64) as usize).min(TEX_SIZE - 1);
    let ty = ((v * TEX_SIZE as f64) as usize).min(TEX_SIZE - 1);
    tex.data[ty * TEX_SIZE + tx]
}

pub(crate) fn sample_tex_shaded(tex: &TextureBuffer, u: f64, v: f64, shade: f64) -> [u8; 4] {
    let [r, g, b, a] = sample_tex(tex, u, v);
    [
        (r as f64 * shade) as u8,
        (g as f64 * shade) as u8,
        (b as f64 * shade) as u8,
        a,
    ]
}

pub(crate) fn set_pixel(buf: &mut [u8], stride: usize, x: usize, y: usize, c: [u8; 4]) {
    let i = (y * stride + x) * 4;
    buf[i..i + 4].copy_from_slice(&c);
}

fn get_2d_ctx(canvas: &web_sys::HtmlCanvasElement) -> Option<web_sys::CanvasRenderingContext2d> {
    canvas
        .get_context("2d")
        .ok()?
        .map(|ctx| ctx.unchecked_into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::{TextureBuffer, TEX_SIZE};
    use wasm_bindgen_test::*;

    /// Helper: create a texture where pixel (x, y) has a unique color.
    fn make_gradient_tex() -> TextureBuffer {
        let mut tex = TextureBuffer::new();
        for y in 0..TEX_SIZE {
            for x in 0..TEX_SIZE {
                tex.set(x, y, [x as u8 * 16, y as u8 * 16, 128, 255]);
            }
        }
        tex
    }

    #[wasm_bindgen_test]
    fn sample_tex_corners() {
        let tex = make_gradient_tex();
        // (0.0, 0.0) maps to pixel (0, 0)
        assert_eq!(sample_tex(&tex, 0.0, 0.0), tex.data[0]);
        // (0.99, 0.99) maps to pixel (15, 15)
        assert_eq!(sample_tex(&tex, 0.99, 0.99), tex.data[15 * TEX_SIZE + 15]);
    }

    #[wasm_bindgen_test]
    fn sample_tex_shaded_halves() {
        let mut tex = TextureBuffer::new();
        tex.set(0, 0, [200, 100, 50, 255]);

        let c = sample_tex_shaded(&tex, 0.0, 0.0, 0.5);
        assert_eq!(c[0], 100); // 200 * 0.5
        assert_eq!(c[1], 50); // 100 * 0.5
        assert_eq!(c[2], 25); // 50 * 0.5
        assert_eq!(c[3], 255); // alpha preserved
    }

    #[wasm_bindgen_test]
    fn set_pixel_correct_offset() {
        let stride = 10;
        let mut buf = vec![0u8; stride * stride * 4];
        set_pixel(&mut buf, stride, 3, 2, [11, 22, 33, 44]);

        let i = (2 * stride + 3) * 4;
        assert_eq!(&buf[i..i + 4], &[11, 22, 33, 44]);
    }

    #[wasm_bindgen_test]
    fn tiled_pixels_wraps() {
        let tex = make_gradient_tex();
        let pixels = tiled_pixels(&tex);
        let size = TEX_SIZE * 4;

        // Pixel at (TEX_SIZE, 0) should equal pixel at (0, 0) due to wrapping
        let idx_wrap = TEX_SIZE * 4;
        let idx_orig = 0;
        assert_eq!(
            &pixels[idx_wrap..idx_wrap + 4],
            &pixels[idx_orig..idx_orig + 4],
        );

        // Pixel at (0, TEX_SIZE) should equal pixel at (0, 0)
        let idx_wrap_y = (TEX_SIZE * size) * 4;
        assert_eq!(
            &pixels[idx_wrap_y..idx_wrap_y + 4],
            &pixels[idx_orig..idx_orig + 4],
        );
    }

    #[wasm_bindgen_test]
    fn isometric_center_covered() {
        let tex = make_gradient_tex();
        let pixels = isometric_pixels(&tex);
        let w = 96;

        // Pixel at (48, 24) is the center of the top face — should be non-transparent
        let i = (24 * w + 48) * 4;
        let alpha = pixels[i + 3];
        assert_ne!(alpha, 0, "center pixel (48,24) should be non-transparent");
    }
}
