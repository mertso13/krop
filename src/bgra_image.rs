pub const BYTES_PER_PIXEL: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BgraImage {
    width: u32,
    height: u32,
    top_down_pixels: Vec<u8>,
}

impl BgraImage {
    pub fn new(width: u32, height: u32, top_down_pixels: Vec<u8>) -> Self {
        assert_eq!(
            top_down_pixels.len(),
            width as usize * height as usize * BYTES_PER_PIXEL,
            "BGRA pixel buffer length must match width * height * 4"
        );
        Self {
            width,
            height,
            top_down_pixels,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn top_down_pixels(&self) -> &[u8] {
        &self.top_down_pixels
    }

    pub fn row_stride(&self) -> usize {
        self.width as usize * BYTES_PER_PIXEL
    }
}
