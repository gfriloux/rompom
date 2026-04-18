{pkgs, ...}:
pkgs.runCommand "deadnix-check" {} ''
  ${pkgs.deadnix}/bin/deadnix --fail ${./../..}
  touch $out
''
