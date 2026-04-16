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

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn new_correct_dimensions() {
        let buf = TextureBuffer::new();
        assert_eq!(buf.data.len(), TEX_SIZE * TEX_SIZE);
        for px in &buf.data {
            assert_eq!(*px, [0, 0, 0, 255]);
        }
    }

    #[wasm_bindgen_test]
    fn set_writes_correct_pixel() {
        let mut buf = TextureBuffer::new();
        buf.set(3, 5, [255, 0, 0, 255]);
        assert_eq!(buf.data[5 * TEX_SIZE + 3], [255, 0, 0, 255]);
    }

    #[wasm_bindgen_test]
    fn set_corners() {
        let mut buf = TextureBuffer::new();
        buf.set(0, 0, [1, 2, 3, 4]);
        buf.set(15, 0, [5, 6, 7, 8]);
        buf.set(0, 15, [9, 10, 11, 12]);
        buf.set(15, 15, [13, 14, 15, 16]);

        assert_eq!(buf.data[0], [1, 2, 3, 4]);
        assert_eq!(buf.data[15], [5, 6, 7, 8]);
        assert_eq!(buf.data[15 * TEX_SIZE], [9, 10, 11, 12]);
        assert_eq!(buf.data[15 * TEX_SIZE + 15], [13, 14, 15, 16]);
    }

    #[wasm_bindgen_test]
    fn as_u8_slice_length() {
        let buf = TextureBuffer::new();
        assert_eq!(buf.as_u8_slice().len(), TEX_SIZE * TEX_SIZE * 4);
    }

    #[wasm_bindgen_test]
    fn as_u8_slice_rgba_order() {
        let mut buf = TextureBuffer::new();
        buf.set(0, 0, [10, 20, 30, 40]);
        let bytes = buf.as_u8_slice();
        assert_eq!(bytes[0], 10);
        assert_eq!(bytes[1], 20);
        assert_eq!(bytes[2], 30);
        assert_eq!(bytes[3], 40);
    }

    #[wasm_bindgen_test]
    fn clone_independence() {
        let mut original = TextureBuffer::new();
        original.set(0, 0, [100, 200, 50, 255]);

        let mut cloned = original.clone();
        cloned.set(0, 0, [0, 0, 0, 0]);

        assert_eq!(original.data[0], [100, 200, 50, 255]);
        assert_eq!(cloned.data[0], [0, 0, 0, 0]);
    }
}
