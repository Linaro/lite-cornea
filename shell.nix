let
  pkgs = import <nixpkgs> {};
in
pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    rustfmt
    cargo
    cargo-watch
    gnuplot
    gdb
    lsof # Helpful to determine FVP port
  ];
}
