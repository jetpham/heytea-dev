{
  description = "heytea.dev singleton wait-time dashboard";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    deploy-rs.url = "github:serokell/deploy-rs";
    agenix.url = "github:ryantm/agenix";
  };

  outputs = inputs@{ self, nixpkgs, flake-utils, deploy-rs, agenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            pkg-config
            openssl
            nodejs_22
            pnpm
            opentofu
            sqlx-cli
            postgresql_16
            redis
            jq
            gh
            deploy-rs.packages.${system}.deploy-rs
            agenix.packages.${system}.default
          ];

          shellHook = ''
            export SQLX_OFFLINE=true
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      }) // {
        nixosModules.heytea = import ./nix/modules/heytea.nix;

        nixosConfigurations.heytea = nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          modules = [
            agenix.nixosModules.default
            self.nixosModules.heytea
            ./nix/hosts/heytea.nix
          ];
        };

        deploy.nodes.heytea = {
          hostname = "heytea-vps";
          profiles.system = {
            user = "root";
            path = deploy-rs.lib.x86_64-linux.activate.nixos self.nixosConfigurations.heytea;
          };
        };
      };
}
