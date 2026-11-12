defmodule ExTerrarium.MixProject do
  use Mix.Project

  @version "0.1.0"
  @source_url "https://github.com/cortfritz/ex_terrarium"

  def project do
    [
      app: :ex_terrarium,
      version: @version,
      elixir: "~> 1.18",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      description: description(),
      package: package(),
      docs: [main: "readme", extras: ["README.md"], source_ref: "v#{@version}"],
      source_url: @source_url
    ]
  end

  def application do
    [
      extra_applications: [:logger]
    ]
  end

  defp deps do
    [
      {:rustler, "~> 0.37", runtime: false},
      {:rustler_precompiled, "~> 0.8"},
      {:ex_doc, ">= 0.0.0", only: :dev, runtime: false}
    ]
  end

  defp description do
    "Decode Terrarium-encoded elevation tiles (PNG and WebP) and sample terrain " <>
      "heights by latitude/longitude. Safe-Rust NIF: the decoders and tile math " <>
      "contain no unsafe code."
  end

  defp package do
    [
      name: "ex_terrarium",
      files: [
        "lib",
        "native/terrarium_nif/src",
        "native/terrarium_nif/Cargo.toml",
        "native/terrarium_nif/Cargo.lock",
        "checksum-*.exs",
        "mix.exs",
        "README.md",
        "LICENSE*"
      ],
      maintainers: ["Cort Fritz"],
      licenses: ["MIT"],
      links: %{
        "GitHub" => @source_url,
        "Sponsor" => "https://github.com/sponsors/cortfritz"
      }
    ]
  end
end
