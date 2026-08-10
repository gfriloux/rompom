{
  pkgs,
  lib,
  ...
}:
pkgs.pkgsStatic.rustPlatform.buildRustPackage {
  pname = "rompom";
  version = "0.16.0";

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
      "screenscraper-0.7.0" = "sha256-iQpVyZmBXB90N+93Waoo0yDIc0+UZR+xJc61S9ETdoI=";
    };
  };

  # perl is required by openssl-src to build OpenSSL from source (vendored feature)
  nativeBuildInputs = [pkgs.perl];

  # Size optimisations (release profile overrides).
  #
  # panic=abort is deliberately NOT set: the worker pool catches a panicking step
  # handler and turns it into a failed ROM so the rest of the run completes. Aborting
  # would kill the process instead, and the catch_unwind would be dead code in exactly
  # the build users run.
  #
  # Measured on 2026-08-09, x86_64-unknown-linux-musl: 11 117 480 bytes with
  # panic=abort against 11 600 808 without, so unwinding costs 483 KB (+4.3%) on a
  # binary already dominated by vendored OpenSSL. That is the trade this package
  # accepts — a run that dies halfway through costs more than 4% of size.
  env = {
    CARGO_PROFILE_RELEASE_OPT_LEVEL = "z";
    CARGO_PROFILE_RELEASE_LTO = "thin";
    CARGO_PROFILE_RELEASE_STRIP = "symbols";
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
