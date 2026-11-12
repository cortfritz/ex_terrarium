//! NIF glue for `ExTerrarium.Native`. All decoding and math lives in the
//! `terrarium` module, which forbids `unsafe`; this file only converts terms.

pub mod terrarium;

use rustler::{Atom, Binary, ResourceArc};
use terrarium::{DecodeError, Grid, Interpolation};

mod atoms {
    rustler::atoms! {
        unknown_format,
        invalid_image,
        too_large,
        unsupported_color,
        invalid_coordinates,
        nearest,
        bilinear,
        invalid_interpolation,
    }
}

pub struct TileResource(Grid);

#[rustler::resource_impl]
impl rustler::Resource for TileResource {}

fn error_atom(e: DecodeError) -> Atom {
    match e {
        DecodeError::UnknownFormat => atoms::unknown_format(),
        DecodeError::InvalidImage => atoms::invalid_image(),
        DecodeError::TooLarge => atoms::too_large(),
        DecodeError::UnsupportedColor => atoms::unsupported_color(),
    }
}

/// Decoding a 512px tile takes a few milliseconds, so it runs on a dirty CPU scheduler.
#[rustler::nif(schedule = "DirtyCpu")]
fn decode(bytes: Binary) -> Result<ResourceArc<TileResource>, Atom> {
    terrarium::decode(bytes.as_slice())
        .map(|grid| ResourceArc::new(TileResource(grid)))
        .map_err(error_atom)
}

#[rustler::nif]
fn info(tile: ResourceArc<TileResource>) -> (u32, u32, usize) {
    (tile.0.width, tile.0.height, tile.0.byte_size())
}

#[rustler::nif]
fn tile_for(lat: f64, lon: f64, zoom: u32) -> Result<(u32, u32), Atom> {
    terrarium::tile_for(lat, lon, zoom).ok_or_else(atoms::invalid_coordinates)
}

#[rustler::nif]
fn sample(
    tile: ResourceArc<TileResource>,
    zoom: u32,
    x: u32,
    y: u32,
    points: Vec<(f64, f64)>,
    interpolation: Atom,
) -> Result<Vec<Option<f64>>, Atom> {
    let interpolation = if interpolation == atoms::nearest() {
        Interpolation::Nearest
    } else if interpolation == atoms::bilinear() {
        Interpolation::Bilinear
    } else {
        return Err(atoms::invalid_interpolation());
    };
    Ok(points
        .into_iter()
        .map(|(lat, lon)| tile.0.sample(zoom, x, y, lat, lon, interpolation))
        .collect())
}

rustler::init!("Elixir.ExTerrarium.Native");
