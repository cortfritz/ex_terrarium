defmodule ExTerrarium do
  @moduledoc """
  Decode Terrarium-encoded elevation tiles and sample terrain heights.

  [Terrarium](https://github.com/tilezen/joerd/blob/master/docs/formats.md#terrarium)
  stores elevation in a tile's RGB channels as `R * 256 + G + B / 256 - 32768`
  metres. This library decodes such tiles (PNG, or lossy/lossless WebP) into an
  opaque height grid held on the Rust side, then samples it by latitude and
  longitude using Web Mercator (slippy map) tile math.

  The tile *bytes* are the caller's business: fetch them from a PMTiles
  archive, a tile server or disk, then hand them to `decode/1`.

      {:ok, {x, y}} = ExTerrarium.tile_for(46.55, 7.98, 12)
      bytes = MyTiles.fetch!(12, x, y)
      {:ok, tile} = ExTerrarium.decode(bytes)
      {:ok, [h1, h2]} = ExTerrarium.sample(tile, {12, x, y}, [{46.55, 7.98}, {46.56, 7.99}])

  ## Safety

  The decoders (`image-webp`, `png`) and this library's tile math are pure
  Rust with `unsafe` forbidden, so a malformed tile returns an error instead of
  corrupting the VM. Images over 4096x4096 pixels are refused before their
  pixel buffer is allocated. `decode/1` runs on a dirty CPU scheduler.
  """

  alias ExTerrarium.Native

  @typedoc "A decoded tile. Opaque; memory is released when the term is garbage collected."
  @opaque tile :: reference()

  @typedoc "Tile address: `{zoom, x, y}`."
  @type tile_id :: {non_neg_integer(), non_neg_integer(), non_neg_integer()}

  @type interpolation :: :nearest | :bilinear

  @type decode_error :: :unknown_format | :invalid_image | :too_large | :unsupported_color

  @interpolations [:nearest, :bilinear]

  @doc """
  Decode a Terrarium PNG or WebP tile.

  Returns `{:error, reason}` for bytes that are not a PNG/WebP
  (`:unknown_format`), are corrupt or truncated (`:invalid_image`), exceed
  4096x4096 pixels (`:too_large`), or are not RGB/RGBA (`:unsupported_color`).
  """
  @spec decode(binary()) :: {:ok, tile()} | {:error, decode_error()}
  def decode(bytes) when is_binary(bytes), do: Native.decode(bytes)

  @doc """
  Width, height and resident size in bytes of a decoded tile's height grid.
  """
  @spec info(tile()) ::
          {:ok, %{width: pos_integer(), height: pos_integer(), bytes: pos_integer()}}
  def info(tile) do
    {width, height, bytes} = Native.info(tile)
    {:ok, %{width: width, height: height, bytes: bytes}}
  end

  @doc """
  The `{x, y}` of the tile containing a point at `zoom` (0..24).

  Latitudes beyond the Web Mercator limit (about ±85.05°) clamp to the edge row.
  """
  @spec tile_for(number(), number(), non_neg_integer()) ::
          {:ok, {non_neg_integer(), non_neg_integer()}} | {:error, :invalid_coordinates}
  def tile_for(lat, lon, zoom)
      when is_number(lat) and is_number(lon) and is_integer(zoom) and zoom >= 0 do
    Native.tile_for(lat * 1.0, lon * 1.0, zoom)
  end

  def tile_for(_lat, _lon, _zoom), do: {:error, :invalid_coordinates}

  @doc """
  Heights in metres for `{lat, lon}` points, in order.

  A point outside the tile (or with an invalid coordinate) yields `nil` in its
  position, so callers can group points by `tile_for/3` and sample each group
  against its own tile.

  ## Options

    * `:interpolation` - `:nearest` (default) reads the containing pixel;
      `:bilinear` blends the four nearest pixel centres, clamping at the tile
      edge.
  """
  @spec sample(tile(), tile_id(), [{number(), number()}], keyword()) ::
          {:ok, [float() | nil]} | {:error, :invalid_interpolation | :invalid_points}
  def sample(tile, {zoom, x, y}, points, opts \\ [])
      when is_integer(zoom) and zoom >= 0 and is_integer(x) and x >= 0 and is_integer(y) and
             y >= 0 and is_list(points) and is_list(opts) do
    interpolation = Keyword.get(opts, :interpolation, :nearest)

    with true <- interpolation in @interpolations || {:error, :invalid_interpolation},
         {:ok, floats} <- normalize_points(points) do
      Native.sample(tile, zoom, x, y, floats, interpolation)
    end
  end

  @doc """
  Height in metres at one point. `{:error, :outside_tile}` when the point is not in the tile.
  """
  @spec elevation(tile(), tile_id(), number(), number(), keyword()) ::
          {:ok, float()}
          | {:error, :outside_tile | :invalid_interpolation | :invalid_points}
  def elevation(tile, tile_id, lat, lon, opts \\ []) do
    case sample(tile, tile_id, [{lat, lon}], opts) do
      {:ok, [nil]} -> {:error, :outside_tile}
      {:ok, [height]} -> {:ok, height}
      error -> error
    end
  end

  defp normalize_points(points) do
    Enum.reduce_while(points, {:ok, []}, fn
      {lat, lon}, {:ok, acc} when is_number(lat) and is_number(lon) ->
        {:cont, {:ok, [{lat * 1.0, lon * 1.0} | acc]}}

      _other, _acc ->
        {:halt, {:error, :invalid_points}}
    end)
    |> case do
      {:ok, acc} -> {:ok, Enum.reverse(acc)}
      error -> error
    end
  end
end
