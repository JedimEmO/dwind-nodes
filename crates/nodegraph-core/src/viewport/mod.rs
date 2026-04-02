/// Viewport manages pan/zoom state and coordinate transforms between screen and world space.
///
/// Screen space: pixel coordinates relative to the viewport container (0,0 = top-left).
/// World space: the infinite canvas where nodes are positioned.
///
/// The transform is: screen = world * zoom + pan
/// Inverse:          world = (screen - pan) / zoom
pub struct Viewport {
    pub pan: (f64, f64),
    pub zoom: f64,
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            pan: (0.0, 0.0),
            zoom: 1.0,
        }
    }

    pub fn screen_to_world(&self, screen_x: f64, screen_y: f64) -> (f64, f64) {
        (
            (screen_x - self.pan.0) / self.zoom,
            (screen_y - self.pan.1) / self.zoom,
        )
    }

    pub fn world_to_screen(&self, world_x: f64, world_y: f64) -> (f64, f64) {
        (
            world_x * self.zoom + self.pan.0,
            world_y * self.zoom + self.pan.1,
        )
    }

    /// Zoom to `new_zoom` while keeping the world point under (screen_x, screen_y) fixed.
    pub fn zoom_at(&mut self, screen_x: f64, screen_y: f64, new_zoom: f64) {
        let new_zoom = new_zoom.clamp(0.1, 10.0);
        // World point under cursor before zoom
        let (wx, wy) = self.screen_to_world(screen_x, screen_y);
        self.zoom = new_zoom;
        // Adjust pan so the same world point maps back to the same screen point
        self.pan.0 = screen_x - wx * self.zoom;
        self.pan.1 = screen_y - wy * self.zoom;
    }

    pub fn pan_by(&mut self, dx: f64, dy: f64) {
        self.pan.0 += dx;
        self.pan.1 += dy;
    }

    /// Adjust pan and zoom so that the given world-space bounds fit within the viewport.
    /// `bounds` is (x, y, width, height) in world space.
    /// `viewport_size` is (width, height) in screen pixels.
    pub fn fit_to_bounds(&mut self, bounds: (f64, f64, f64, f64), viewport_size: (f64, f64)) {
        let (bx, by, bw, bh) = bounds;
        if bw <= 0.0 || bh <= 0.0 || viewport_size.0 <= 0.0 || viewport_size.1 <= 0.0 {
            return;
        }

        let padding = 50.0; // screen-space padding
        let available_w = viewport_size.0 - padding * 2.0;
        let available_h = viewport_size.1 - padding * 2.0;

        let zoom_x = available_w / bw;
        let zoom_y = available_h / bh;
        self.zoom = zoom_x.min(zoom_y).clamp(0.1, 10.0);

        // Center the bounds in the viewport
        let center_wx = bx + bw / 2.0;
        let center_wy = by + bh / 2.0;
        self.pan.0 = viewport_size.0 / 2.0 - center_wx * self.zoom;
        self.pan.1 = viewport_size.1 / 2.0 - center_wy * self.zoom;
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}
