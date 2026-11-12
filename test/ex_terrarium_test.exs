defmodule ExTerrariumTest do
  use ExUnit.Case, async: true

  # Fixtures are 64x64 synthetic tiles (see native/terrarium_nif/examples/make_fixtures.rs):
  # pixel (px, py) holds `px * 10 + py - 400` metres.
  @fixtures Path.expand("fixtures", __DIR__)
  @size 64
  @tile_id {12, 682, 1649}

  defp fixture(name), do: File.read!(Path.join(@fixtures, name))

  defp expected(px, py), do: px * 10.0 + py - 400.0

  # Latitude/longitude of a fractional position inside @tile_id, in pixels.
  defp point_at({z, x, y}, px, py) do
    n = :math.pow(2, z)
    fx = x + px / @size
    fy = y + py / @size
    lon = fx / n * 360.0 - 180.0
    lat = :math.atan(:math.sinh(:math.pi() * (1 - 2 * fy / n))) * 180.0 / :math.pi()
    {lat, lon}
  end

  describe "decode/1" do
    test "decodes PNG and WebP to the same heights" do
      {:ok, png} = ExTerrarium.decode(fixture("synthetic.png"))
      {:ok, webp} = ExTerrarium.decode(fixture("synthetic.webp"))

      points =
        for px <- [0, 7, 31, 63], py <- [0, 12, 63], do: point_at(@tile_id, px + 0.5, py + 0.5)

      assert {:ok, from_png} = ExTerrarium.sample(png, @tile_id, points)
      assert {:ok, ^from_png} = ExTerrarium.sample(webp, @tile_id, points)

      assert from_png ==
               for(px <- [0, 7, 31, 63], py <- [0, 12, 63], do: expected(px, py))
    end

    test "reports grid size" do
      {:ok, tile} = ExTerrarium.decode(fixture("synthetic.webp"))
      assert {:ok, %{width: 64, height: 64, bytes: 16_384}} = ExTerrarium.info(tile)
    end

    test "rejects non-images, truncated images and non-RGB images" do
      assert {:error, :unknown_format} = ExTerrarium.decode("hello")
      assert {:error, :unknown_format} = ExTerrarium.decode(<<>>)

      png = fixture("synthetic.png")
      assert {:error, :invalid_image} = ExTerrarium.decode(binary_part(png, 0, 60))

      webp = fixture("synthetic.webp")
      assert {:error, :invalid_image} = ExTerrarium.decode(binary_part(webp, 0, 30))

      assert {:error, :unsupported_color} = ExTerrarium.decode(fixture("grayscale.png"))
    end
  end

  describe "tile_for/3" do
    test "finds the containing tile" do
      assert {:ok, {1, 1}} = ExTerrarium.tile_for(0, 0, 1)
      assert {:ok, {2347, 3129}} = ExTerrarium.tile_for(39.036, -76.82, 13)
      {lat, lon} = point_at(@tile_id, 10.5, 20.5)
      assert {:ok, {682, 1649}} = ExTerrarium.tile_for(lat, lon, 12)
    end

    test "keeps the antimeridian and polar edge in range" do
      assert {:ok, {3, 0}} = ExTerrarium.tile_for(89.9, 180.0, 2)
      assert {:ok, {0, 3}} = ExTerrarium.tile_for(-90.0, -180.0, 2)
    end

    test "rejects invalid input" do
      assert {:error, :invalid_coordinates} = ExTerrarium.tile_for(91.0, 0.0, 3)
      assert {:error, :invalid_coordinates} = ExTerrarium.tile_for(0.0, 181.0, 3)
      assert {:error, :invalid_coordinates} = ExTerrarium.tile_for(0.0, 0.0, 25)
      assert {:error, :invalid_coordinates} = ExTerrarium.tile_for("0", 0.0, 3)
      assert {:error, :invalid_coordinates} = ExTerrarium.tile_for(0.0, 0.0, -1)
    end
  end

  describe "sample/4" do
    setup do
      {:ok, tile} = ExTerrarium.decode(fixture("synthetic.webp"))
      %{tile: tile}
    end

    test "nearest reads the containing pixel", %{tile: tile} do
      {lat, lon} = point_at(@tile_id, 5.9, 3.1)
      assert {:ok, [height]} = ExTerrarium.sample(tile, @tile_id, [{lat, lon}])
      assert height == expected(5, 3)
    end

    test "bilinear blends neighbouring pixel centres", %{tile: tile} do
      # Halfway between the centres of pixels (5, 3) and (6, 3).
      {lat, lon} = point_at(@tile_id, 6.0, 3.5)

      assert {:ok, [height]} =
               ExTerrarium.sample(tile, @tile_id, [{lat, lon}], interpolation: :bilinear)

      assert_in_delta height, (expected(5, 3) + expected(6, 3)) / 2, 1.0e-3
    end

    test "returns nil for points outside the tile and accepts integers", %{tile: tile} do
      inside = point_at(@tile_id, 1.5, 1.5)

      assert {:ok, [h, nil, nil]} =
               ExTerrarium.sample(tile, @tile_id, [inside, {0, 0}, {95.0, 0}])

      assert h == expected(1, 1)
    end

    test "elevation/5 wraps a single point", %{tile: tile} do
      {lat, lon} = point_at(@tile_id, 63.5, 63.5)
      assert {:ok, height} = ExTerrarium.elevation(tile, @tile_id, lat, lon)
      assert height == expected(63, 63)
      assert {:error, :outside_tile} = ExTerrarium.elevation(tile, @tile_id, 0.0, 0.0)
    end

    test "validates options and points", %{tile: tile} do
      assert {:error, :invalid_interpolation} =
               ExTerrarium.sample(tile, @tile_id, [{0.0, 0.0}], interpolation: :cubic)

      assert {:error, :invalid_points} = ExTerrarium.sample(tile, @tile_id, [{0.0, "x"}])
      assert {:error, :invalid_points} = ExTerrarium.sample(tile, @tile_id, [:nope])
      assert {:ok, []} = ExTerrarium.sample(tile, @tile_id, [])
    end
  end
end
