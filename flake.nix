{

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      nixpkgs,
      crane,
      ...
    }:
    let
      forAllSystems =
        function:
        nixpkgs.lib.genAttrs
          [
            "x86_64-linux"
            "aarch64-darwin"
          ]
          (
            system:
            let
              pkgs = import nixpkgs {
                inherit system;
                config = {
                  allowUnfree = true;
                };

              };

              buildInputs = with pkgs; [
                pkg-config
              ];

              nativeBuildInputs = with pkgs; [
                glfw
                cmake
                clang
                cargo
                rustc
              ];

            in
            function {
              inherit
                pkgs
                buildInputs
                nativeBuildInputs
                ;

            }
          );
    in
    {

      #TODO: make a darwin and linux different packages
      packages = forAllSystems (
        {
          pkgs,
          nativeBuildInputs,
          buildInputs,
        }:
        {
          default =
            let
              craneLib = crane.mkLib pkgs;
              lib = pkgs.lib;
            in
            craneLib.buildPackage {
              inherit nativeBuildInputs buildInputs;
              src = ./.;

              env = {
                RUSTFLAGS = "-C link-args=-Wl,-rpath,${lib.makeLibraryPath buildInputs}";
              };

            };
        }
      );

      templates.default.path = ./.;

      devShells = forAllSystems (
        {
          pkgs,
          buildInputs,
          nativeBuildInputs,
        }:
        {
          default =
            let
              LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
            in
            pkgs.mkShell {
              inherit nativeBuildInputs buildInputs LD_LIBRARY_PATH;

              packages = with pkgs; [
                cargo
                bacon
                rust-analyzer
                clippy
                rustfmt
                taplo # lsp for cargo.toml

                claude-code # the demon itself
              ];

            };
        }
      );

    };
}
