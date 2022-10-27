nixpkgs:
nixpkgs.stdenv.mkDerivation {
  name = "cornea-dev-env";
  buildInputs = with nixpkgs; [
    rustc
    rustfmt
    cargo
    cargo-watch
    gnuplot
    gdb
    lsof # Helpful to determine FVP port
  ];
}
