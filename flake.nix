{
  description = "sepiida - adaptive desktop theming engine for Windows";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          # Version, components and the Windows target live in rust-toolchain.toml
          # so that CI (rustup on windows-latest) uses the same toolchain.
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              # Cross-compiling to x86_64-pc-windows-msvc: cargo-xwin fetches the
              # MSVC CRT + Windows SDK and drives clang-cl / lld-link.
              pkgs.cargo-xwin
              pkgs.llvmPackages.clang-unwrapped
              pkgs.llvmPackages.lld
              pkgs.llvmPackages.llvm
              pkgs.slint-lsp
            ];
          };
        }
      );
    };
}
