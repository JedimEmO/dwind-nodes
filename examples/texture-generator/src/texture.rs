pub const TEX_SIZE: usize = 16;

#[derive(Clone)]
pub struct TextureBuffer {
    pub data: Vec<[u8; 4]>,
}

impl TextureBuffer {
    pub fn new() -> Self {
        Self {
            data: vec![[0, 0, 0, 255]; TEX_SIZE * TEX_SIZE],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, color: [u8; 4]) {
        self.data[y * TEX_SIZE + x] = color;
    }

    /// Flatten to a contiguous u8 slice for ImageData (RGBA order).
    pub fn as_u8_slice(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(TEX_SIZE * TEX_SIZE * 4);
        for px in &self.data {
            out.extend_from_slice(px);
        }
        out
    }
}
