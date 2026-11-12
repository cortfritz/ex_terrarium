defmodule ExTerrarium.Native do
  @moduledoc false
  version = Mix.Project.config()[:version]

  use RustlerPrecompiled,
    otp_app: :ex_terrarium,
    crate: "terrarium_nif",
    base_url: "https://github.com/cortfritz/ex_terrarium/releases/download/v#{version}",
    force_build:
      System.get_env("RUSTLER_PRECOMPILATION_EXAMPLE_BUILD") in ["1", "true"] or
        Mix.env() in [:dev, :test],
    version: version

  def decode(_bytes), do: :erlang.nif_error(:nif_not_loaded)
  def info(_tile), do: :erlang.nif_error(:nif_not_loaded)
  def tile_for(_lat, _lon, _zoom), do: :erlang.nif_error(:nif_not_loaded)

  def sample(_tile, _zoom, _x, _y, _points, _interpolation),
    do: :erlang.nif_error(:nif_not_loaded)
end
