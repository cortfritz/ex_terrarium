//! Terrarium DEM tile decoding, Web Mercator tile math and height sampling.
//!
//! Everything here is safe Rust: the module forbids `unsafe`, and both image
//! decoders (`image-webp`, `png`) forbid it in their own crates. The only
//! `unsafe` in this library is the NIF glue Rustler generates in `lib.rs`.
#![forbid(unsafe_code)]

use std::io::Cursor;

/// Largest image accepted, in pixels. A Terrarium tile is 256x256 or 512x512;
/// anything bigger is refused before its pixel buffer is allocated.
pub const MAX_PIXELS: u64 = 4096 * 4096;

/// Deepest zoom accepted by the tile math.
pub const MAX_ZOOM: u32 = 24;

/// Web Mercator latitude limit.
const MAX_LAT: f64 = 85.051_128_779_806_59;

/// A decoded tile: heights in metres, row-major, `width * height` entries.
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub heights: Vec<f32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownFormat,
    InvalidImage,
    TooLarge,
    UnsupportedColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Nearest,
    Bilinear,
}

/// Terrarium encoding: `R * 256 + G + B / 256 - 32768` metres.
#[inline]
pub fn terrarium(r: u8, g: u8, b: u8) -> f32 {
    (f64::from(r) * 256.0 + f64::from(g) + f64::from(b) / 256.0 - 32768.0) as f32
}

/// Decode a Terrarium-encoded PNG or WebP (lossy or lossless) tile.
pub fn decode(bytes: &[u8]) -> Result<Grid, DecodeError> {
    if is_webp(bytes) {
        decode_webp(bytes)
    } else if is_png(bytes) {
        decode_png(bytes)
    } else {
        Err(DecodeError::UnknownFormat)
    }
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn check_size(width: u32, height: u32) -> Result<(), DecodeError> {
    if width == 0 || height == 0 {
        Err(DecodeError::InvalidImage)
    } else if u64::from(width) * u64::from(height) > MAX_PIXELS {
        Err(DecodeError::TooLarge)
    } else {
        Ok(())
    }
}

fn decode_webp(bytes: &[u8]) -> Result<Grid, DecodeError> {
    let mut decoder =
        image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|_| DecodeError::InvalidImage)?;
    let (width, height) = decoder.dimensions();
    check_size(width, height)?;
    let channels = if decoder.has_alpha() { 4 } else { 3 };
    let len = decoder
        .output_buffer_size()
        .ok_or(DecodeError::TooLarge)?;
    let mut buf = vec![0u8; len];
    decoder
        .read_image(&mut buf)
        .map_err(|_| DecodeError::InvalidImage)?;
    to_grid(&buf, channels, width, height)
}

fn decode_png(bytes: &[u8]) -> Result<Grid, DecodeError> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: (MAX_PIXELS * 4) as usize,
        },
    );
    // Palette -> RGB(A), 16-bit -> 8-bit, so every accepted tile reads as RGB(A) bytes.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| DecodeError::InvalidImage)?;
    {
        let info = reader.info();
        check_size(info.width, info.height)?;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|_| DecodeError::InvalidImage)?;
    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(DecodeError::UnsupportedColor),
    };
    buf.truncate(frame.buffer_size());
    to_grid(&buf, channels, frame.width, frame.height)
}

fn to_grid(buf: &[u8], channels: usize, width: u32, height: u32) -> Result<Grid, DecodeError> {
    let pixels = width as usize * height as usize;
    if buf.len() < pixels * channels {
        return Err(DecodeError::InvalidImage);
    }
    let heights = buf
        .chunks_exact(channels)
        .take(pixels)
        .map(|p| terrarium(p[0], p[1], p[2]))
        .collect();
    Ok(Grid {
        width,
        height,
        heights,
    })
}

/// Fractional Web Mercator tile coordinates of a point at `zoom`.
/// `None` for a non-finite or out-of-range coordinate or zoom.
pub fn fractional_tile(lat: f64, lon: f64, zoom: u32) -> Option<(f64, f64)> {
    if zoom > MAX_ZOOM
        || !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    let n = f64::from(1u32 << zoom);
    let lat = lat.clamp(-MAX_LAT, MAX_LAT).to_radians();
    let x = (lon + 180.0) / 360.0 * n;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    // lon = 180 and the latitude limit land exactly on the far edge; keep them in the last tile.
    let edge = n - n * f64::EPSILON;
    Some((x.clamp(0.0, edge), y.clamp(0.0, edge)))
}

/// The tile containing a point at `zoom`.
pub fn tile_for(lat: f64, lon: f64, zoom: u32) -> Option<(u32, u32)> {
    fractional_tile(lat, lon, zoom).map(|(x, y)| (x.floor() as u32, y.floor() as u32))
}

impl Grid {
    #[inline]
    fn at(&self, px: usize, py: usize) -> f32 {
        self.heights[py * self.width as usize + px]
    }

    /// Height at a point, or `None` when the point is not inside tile `(zoom, x, y)`.
    ///
    /// `Nearest` truncates to the containing pixel, the same lookup Augur's
    /// Elixir decoder performed. `Bilinear` blends the four nearest pixel
    /// centres, clamping at the tile edge (no neighbour tile is consulted).
    pub fn sample(
        &self,
        zoom: u32,
        x: u32,
        y: u32,
        lat: f64,
        lon: f64,
        interpolation: Interpolation,
    ) -> Option<f64> {
        let (fx, fy) = fractional_tile(lat, lon, zoom)?;
        if fx.floor() as u32 != x || fy.floor() as u32 != y {
            return None;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let u = (fx - f64::from(x)) * w as f64;
        let v = (fy - f64::from(y)) * h as f64;
        match interpolation {
            Interpolation::Nearest => {
                let px = (u as usize).min(w - 1);
                let py = (v as usize).min(h - 1);
                Some(f64::from(self.at(px, py)))
            }
            Interpolation::Bilinear => {
                let gx = (u - 0.5).clamp(0.0, (w - 1) as f64);
                let gy = (v - 0.5).clamp(0.0, (h - 1) as f64);
                let (x0, y0) = (gx.floor() as usize, gy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                let (tx, ty) = (gx - x0 as f64, gy - y0 as f64);
                let top = f64::from(self.at(x0, y0)) * (1.0 - tx) + f64::from(self.at(x1, y0)) * tx;
                let bottom =
                    f64::from(self.at(x0, y1)) * (1.0 - tx) + f64::from(self.at(x1, y1)) * tx;
                Some(top * (1.0 - ty) + bottom * ty)
            }
        }
    }

    pub fn byte_size(&self) -> usize {
        self.heights.len() * std::mem::size_of::<f32>()
    }
}

/// Encode a height grid as a Terrarium RGB pixel buffer (used by tests and fixtures).
pub fn encode_rgb(heights: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(heights.len() * 3);
    for &h in heights {
        let v = (f64::from(h) + 32768.0).clamp(0.0, 65_535.996);
        let r = (v / 256.0).floor();
        let g = (v - r * 256.0).floor();
        let b = ((v - r * 256.0 - g) * 256.0).floor();
        out.extend_from_slice(&[r as u8, g as u8, b as u8]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(w: u32, h: u32) -> Vec<f32> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| x as f32 * 10.0 + y as f32 - 400.0))
            .collect()
    }

    fn png_bytes(w: u32, h: u32, heights: &[f32]) -> Vec<u8> {
        let mut out = vec![];
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&encode_rgb(heights))
            .unwrap();
        out
    }

    fn webp_bytes(w: u32, h: u32, heights: &[f32]) -> Vec<u8> {
        let mut out = vec![];
        image_webp::WebPEncoder::new(&mut out)
            .encode(&encode_rgb(heights), w, h, image_webp::ColorType::Rgb8)
            .unwrap();
        out
    }

    #[test]
    fn terrarium_formula() {
        assert_eq!(terrarium(128, 0, 0), 0.0);
        assert_eq!(terrarium(128, 100, 128), 100.5);
        assert_eq!(terrarium(126, 50, 0), -462.0);
    }

    #[test]
    fn png_and_webp_round_trip() {
        let heights = synthetic(16, 8);
        for bytes in [png_bytes(16, 8, &heights), webp_bytes(16, 8, &heights)] {
            let grid = decode(&bytes).unwrap();
            assert_eq!((grid.width, grid.height), (16, 8));
            assert_eq!(grid.heights, heights);
        }
    }

    #[test]
    fn rejects_garbage_and_truncation() {
        assert_eq!(decode(b"not an image").err(), Some(DecodeError::UnknownFormat));
        let bytes = png_bytes(16, 8, &synthetic(16, 8));
        assert_eq!(decode(&bytes[..40]).err(), Some(DecodeError::InvalidImage));
        let webp = webp_bytes(16, 8, &synthetic(16, 8));
        assert_eq!(decode(&webp[..30]).err(), Some(DecodeError::InvalidImage));
    }

    #[test]
    fn rejects_grayscale_png() {
        let mut out = vec![];
        let mut enc = png::Encoder::new(&mut out, 4, 4);
        enc.set_color(png::ColorType::Grayscale);
        enc.write_header().unwrap().write_image_data(&[0u8; 16]).unwrap();
        assert_eq!(decode(&out).err(), Some(DecodeError::UnsupportedColor));
    }

    #[test]
    fn tile_math_matches_known_tiles() {
        assert_eq!(tile_for(0.0, 0.0, 1), Some((1, 1)));
        assert_eq!(tile_for(39.036, -76.82, 13), Some((2347, 3129)));
        assert_eq!(tile_for(85.1, 180.0, 2), Some((3, 0)));
        assert_eq!(tile_for(-90.0, -180.0, 2), Some((0, 3)));
        assert_eq!(tile_for(91.0, 0.0, 2), None);
        assert_eq!(tile_for(0.0, f64::NAN, 2), None);
        assert_eq!(tile_for(0.0, 0.0, 25), None);
    }

    #[test]
    fn nearest_and_bilinear_sampling() {
        let (w, h) = (4u32, 4u32);
        let grid = Grid {
            width: w,
            height: h,
            heights: synthetic(w, h),
        };
        // Tile (z=1, x=1, y=1) spans lon 0..180, lat 0..-85.05.
        let (z, x, y) = (1, 1, 1);
        // Centre of pixel (0, 0): a quarter-tile in is lon 22.5 at x; pick lat from the pixel centre.
        let lat_at = |fy: f64| {
            let n = 2f64;
            (std::f64::consts::PI * (1.0 - 2.0 * fy / n)).sinh().atan().to_degrees()
        };
        let lat = lat_at(1.0 + 0.125);
        let lon = 22.5;
        assert_eq!(grid.sample(z, x, y, lat, lon, Interpolation::Nearest), Some(-400.0));
        let centre = grid.sample(z, x, y, lat, lon, Interpolation::Bilinear).unwrap();
        assert!((centre - -400.0).abs() < 1e-6);
        // Halfway between pixel centres (0,0) and (1,0): lon 45 is the shared edge.
        let mid = grid
            .sample(z, x, y, lat, 45.0, Interpolation::Bilinear)
            .unwrap();
        assert!((mid - -395.0).abs() < 1e-6);
        // A point in another tile is not sampled.
        assert_eq!(grid.sample(z, x, y, 10.0, 10.0, Interpolation::Nearest), None);
    }
}
