{
  description = "A very basic flake";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils/master";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: {
      devShell = import ./shell.nix (import nixpkgs { inherit system; });
    });
}
