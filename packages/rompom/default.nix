{
  pkgs,
  lib,
  ...
}:
pkgs.pkgsStatic.rustPlatform.buildRustPackage {
  pname = "rompom";
  version = "0.20.0";

  src = builtins.path {
    path = ../..;
    name = "rompom-source";
    filter = path: _type: let
      base = baseNameOf path;
    in
      base != "target" && base != ".git" && base != ".direnv";
  };

  cargoLock = {
    lockFile = ../../Cargo.lock;
    outputHashes = {
      "internet_archive-0.3.0" = "sha256-c6pciyhMQZU8yglV7r6fwyaeA77E+HfT6a/aXg9Fkk8=";
      "screenscraper-0.8.0" = "sha256-IM+LMZRJugTXI4r6uA1M5y4sTlx+MWfTUBvPVidTovA=";
    };
  };

  # perl is required by openssl-src to build OpenSSL from source (vendored feature)
  nativeBuildInputs = [pkgs.perl];

  # The release profile (opt-level, LTO, codegen-units, strip) lives in [profile.release]
  # in Cargo.toml, along with the reason panic = "abort" must stay out of it. It used to
  # be set here as CARGO_PROFILE_RELEASE_* environment variables, which silently override
  # the manifest — so a cargo build outside Nix produced a different binary, and nothing
  # would have flagged the two definitions drifting apart.

  # Force strip all symbols to minimise binary size
  stripAllList = ["bin"];

  meta = {
    description = "ROM packager: Internet Archive + ScreenScraper → PKGBUILD + EmulationStation XML";
    homepage = "https://github.com/gfriloux/rompom";
    license = lib.licenses.mit;
    maintainers = [];
    mainProgram = "rompom";
  };
}
