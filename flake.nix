{
  description = "Swarm Protocol development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:

    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustup
            protobuf
          ];

          TEMPDIR = "/tmp/rust/shell";
          TMPDIR = "/tmp/rust/shell";
          TEMP = "/tmp/rust/shell";
          TMP = "/tmp/rust/shell";

          shellHook = ''
            DATA_DIR="/tmp/rust"
            export RUSTUP_HOME="$DATA_DIR/rustup"
            export CARGO_HOME="$DATA_DIR/cargo"
            export PATH=$CARGO_HOME/bin:$PATH
            mkdir -p "$CARGO_HOME"
            mkdir -p "$RUSTUP_HOME"
            mkdir -p "$DATA_DIR/shell"

            rustup default stable
            rustup toolchain install
            rustup update

            echo
            echo
            echo

            echo "Rustup installed at $RUSTUP_HOME"
            echo "Cargo installed at $CARGO_HOME"
            echo "$(rustup --version)"
            echo "$(cargo --version)"
          '';
        };
      }
    );
}
