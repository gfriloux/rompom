{pkgs, ...}:
pkgs.runCommand "statix-check" {} ''
  ${pkgs.statix}/bin/statix check ${./../..}
  touch $out
''
