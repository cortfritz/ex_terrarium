//! Writes the synthetic Terrarium fixtures used by the Elixir test suite:
//! `cargo run --example make_fixtures -- ../../test/fixtures`
//!
//! Each fixture is a 64x64 tile whose pixel (px, py) holds `px * 10 + py - 400`
//! metres, so tests can assert exact heights without third-party map data.
use terrarium_nif::terrarium::encode_rgb;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "test/fixtures".into());
    let (w, h) = (64u32, 64u32);
    let heights: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| x as f32 * 10.0 + y as f32 - 400.0))
        .collect();
    let rgb = encode_rgb(&heights);

    let mut png_out = vec![];
    let mut enc = png::Encoder::new(&mut png_out, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    std::fs::write(format!("{dir}/synthetic.png"), png_out).unwrap();

    let mut webp_out = vec![];
    image_webp::WebPEncoder::new(&mut webp_out)
        .encode(&rgb, w, h, image_webp::ColorType::Rgb8)
        .unwrap();
    std::fs::write(format!("{dir}/synthetic.webp"), webp_out).unwrap();

    let mut gray_out = vec![];
    let mut enc = png::Encoder::new(&mut gray_out, 4, 4);
    enc.set_color(png::ColorType::Grayscale);
    enc.write_header().unwrap().write_image_data(&[0u8; 16]).unwrap();
    std::fs::write(format!("{dir}/grayscale.png"), gray_out).unwrap();
}
