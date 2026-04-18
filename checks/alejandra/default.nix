{pkgs, ...}:
pkgs.runCommand "alejandra-check" {} ''
  ${pkgs.alejandra}/bin/alejandra --check ${./../..}
  touch $out
''
