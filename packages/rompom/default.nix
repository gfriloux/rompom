{
  pkgs,
  lib,
  ...
}:
pkgs.pkgsStatic.rustPlatform.buildRustPackage {
  pname = "rompom";
  version = "0.13.0";

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
      "internet_archive-0.2.0" = "sha256-W80Y7x0e1t5zpMSdE6mVXqzF7088CJh3UwaRPcTV658=";
      "screenscraper-0.6.0" = "sha256-FIFSnDOjIycPYLgRkyfA0OY8jCIV9/W5kVto0+zeMAk=";
    };
  };

  # perl is required by openssl-src to build OpenSSL from source (vendored feature)
  nativeBuildInputs = [pkgs.perl];

  # Size optimisations (release profile overrides)
  env = {
    CARGO_PROFILE_RELEASE_OPT_LEVEL = "z";
    CARGO_PROFILE_RELEASE_LTO = "thin";
    CARGO_PROFILE_RELEASE_STRIP = "symbols";
    CARGO_PROFILE_RELEASE_PANIC = "abort";
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
  };

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
