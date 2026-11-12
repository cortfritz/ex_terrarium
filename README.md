# ExTerrarium

Decode [Terrarium](https://github.com/tilezen/joerd/blob/master/docs/formats.md#terrarium)-encoded
elevation tiles (PNG or WebP) and sample terrain heights by latitude and longitude, from Elixir.

Terrarium packs elevation into a tile's RGB channels as `R * 256 + G + B / 256 - 32768` metres.
It is the encoding used by [Mapterhorn](https://mapterhorn.com), the
[AWS Terrain Tiles](https://registry.opendata.aws/terrain-tiles/) open dataset and MapLibre's
`raster-dem` sources.

## Why a NIF

Decoding an image tile is the expensive part of an elevation lookup. On real tiles:

| Tile | Pure-Elixir PNG decoder | ExTerrarium |
|---|---|---|
| 256px Terrarium PNG (AWS) | ~14.7 ms | ~0.08 ms warm |
| 512px Terrarium lossless WebP (Mapterhorn) | n/a | ~2.9 ms warm |

(Apple Silicon, release build. Decode once and keep the tile: sampling is a lookup.)

## Safety

The VM's safety matters more than raw speed here, so everything that touches tile bytes is
safe Rust:

- Decoders are [`image-webp`](https://crates.io/crates/image-webp) and
  [`png`](https://crates.io/crates/png), both pure Rust with `#![forbid(unsafe_code)]`.
  No C libraries are linked.
- This library's decoding, tile math and sampling live in a module that also forbids
  `unsafe`; the only `unsafe` is the NIF glue Rustler generates.
- Malformed or truncated input returns `{:error, reason}`. Images larger than 4096x4096
  are refused before their pixel buffer is allocated.
- `decode/1` runs on a dirty CPU scheduler.

## Usage

```elixir
# Find the tile for a point, fetch its bytes from wherever you keep tiles
{:ok, {x, y}} = ExTerrarium.tile_for(46.55, 7.98, 12)
bytes = MyTiles.fetch!(12, x, y)

# Decode once (keep the tile around; it is an opaque reference to a Rust-side height grid)
{:ok, tile} = ExTerrarium.decode(bytes)
{:ok, %{width: 512, height: 512, bytes: 1_048_576}} = ExTerrarium.info(tile)

# Sample many points in one call; points outside the tile come back as nil
{:ok, [h1, h2, nil]} =
  ExTerrarium.sample(tile, {12, x, y}, [{46.55, 7.98}, {46.56, 7.99}, {0.0, 0.0}])

# Bilinear interpolation between pixel centres (default is :nearest)
{:ok, h} = ExTerrarium.elevation(tile, {12, x, y}, 46.55, 7.98, interpolation: :bilinear)
```

Heights are metres relative to the dataset's vertical datum. Datasets differ in how they treat
water: Mapterhorn reports 0 m over the sea, while AWS Terrain Tiles include bathymetry.

## Installation

```elixir
def deps do
  [
    {:ex_terrarium, "~> 0.1"}
  ]
end
```

Precompiled NIFs are downloaded for common targets. To build from source, install Rust
(1.91 or newer) and set `RUSTLER_PRECOMPILATION_EXAMPLE_BUILD=1`.

## Development

```bash
mix deps.get
mix test                                   # builds the NIF from source in dev/test
(cd native/terrarium_nif && cargo test)    # Rust unit tests
# regenerate the synthetic test fixtures
(cd native/terrarium_nif && cargo run --example make_fixtures -- ../../test/fixtures)
```

## License

MIT
