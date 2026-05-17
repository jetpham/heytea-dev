{
  description = "heytea.dev singleton wait-time dashboard";

  nixConfig = {
    max-jobs = "auto";
    cores = 0;
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    deploy-rs.url = "github:serokell/deploy-rs";
    agenix.url = "github:ryantm/agenix";
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ self, nixpkgs, flake-utils, deploy-rs, agenix, disko }:
    let
      serverSystem = "x86_64-linux";
    in
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = import nixpkgs { inherit system; };
          version = "0.1.0";

          rustBinary = { package, pname ? package, extraNativeBuildInputs ? [ ], extraAttrs ? { } }:
            pkgs.rustPlatform.buildRustPackage ({
              inherit pname version;
              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;
              cargoBuildFlags = [ "-p" package ];
              cargoTestFlags = [ "-p" package ];
              nativeBuildInputs = [ pkgs.pkg-config ] ++ extraNativeBuildInputs;
              buildInputs = [ pkgs.openssl ];
            } // extraAttrs);

          assets = pkgs.stdenvNoCC.mkDerivation {
            pname = "heytea-assets";
            inherit version;
            dontUnpack = true;

            nativeBuildInputs = [
              (pkgs.python3.withPackages (ps: [
                ps.brotli
                ps.fonttools
              ]))
            ];

            installPhase = ''
              runHook preInstall
              mkdir -p $out
              pyftsubset ${pkgs.atkinson-hyperlegible}/share/fonts/opentype/AtkinsonHyperlegible-Regular.otf \
                --output-file=$out/a.woff2 \
                --flavor=woff2 \
                --unicodes=U+0020-007E \
                --no-hinting \
                --layout-features='*'
              runHook postInstall
            '';
          };

          migrations = pkgs.stdenvNoCC.mkDerivation {
            pname = "heytea-migrations";
            inherit version;
            src = ./migrations;

            installPhase = ''
              runHook preInstall
              mkdir -p $out/share/heytea/migrations
              cp ./*.sql $out/share/heytea/migrations/
              runHook postInstall
            '';
          };

          assetSrc = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./apps/assets
              ./package.json
              ./pnpm-lock.yaml
              ./pnpm-workspace.yaml
            ];
          };

          siteAssets = pkgs.stdenvNoCC.mkDerivation {
            pname = "heytea-site-assets";
            inherit version;
            src = assetSrc;

            pnpmDeps = pkgs.fetchPnpmDeps {
              pname = "heytea-site-assets-pnpm-deps";
              inherit version;
              src = assetSrc;
              fetcherVersion = 2;
              hash = "sha256-erLuyObbKRe0xHVjR0vM+Bikdvy53OZmdB2gwzRSxTk=";
            };

            nativeBuildInputs = [
              pkgs.nodejs_22
              pkgs.pnpm
              pkgs.pnpmConfigHook
              (pkgs.python3.withPackages (ps: [
                ps.brotli
                ps.fonttools
              ]))
            ];

            buildPhase = ''
              runHook preBuild
              pnpm --filter @heytea/assets build
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p $out
              cp -R apps/assets/dist/* $out/
              pyftsubset ${pkgs.atkinson-hyperlegible}/share/fonts/opentype/AtkinsonHyperlegible-Regular.otf \
                --output-file=$out/a.woff2 \
                --flavor=woff2 \
                --unicodes=U+0020-007E \
                --no-hinting \
                --layout-features='*'
              runHook postInstall
            '';
          };

          dashboardFontTools = pkgs.python3.withPackages (ps: [
            ps.brotli
            ps.fonttools
          ]);

          siteBinary = rustBinary {
            package = "heytea-site";
            extraNativeBuildInputs = [ dashboardFontTools ];
            extraAttrs.ATKINSON_FONT = "${pkgs.atkinson-hyperlegible}/share/fonts/opentype/AtkinsonHyperlegible-Regular.otf";
          };

          site = pkgs.stdenvNoCC.mkDerivation {
            pname = "heytea-site";
            inherit version;
            dontUnpack = true;
            nativeBuildInputs = [ pkgs.makeWrapper ];

            installPhase = ''
              runHook preInstall
              mkdir -p $out/bin $out/share/heytea-site
              cp -R ${siteAssets}/* $out/share/heytea-site/
              makeWrapper ${siteBinary}/bin/heytea-site $out/bin/heytea-site \
                --set-default HEYTEA_SITE_ASSETS_DIR $out/share/heytea-site
              runHook postInstall
            '';
          };

          digitalOceanQcow2Image = pkgs.runCommand "heytea-dev-digital-ocean-qcow2-image" { nativeBuildInputs = [ pkgs.gzip ]; } ''
            runHook preInstall

            mkdir -p $out/nix-support
            gzip -dc ${self.nixosConfigurations.heytea-bootstrap.config.system.build.images."digital-ocean"}/*.qcow2.gz \
              > $out/heytea-dev-digital-ocean.qcow2
            echo "file qcow2-image $out/heytea-dev-digital-ocean.qcow2" \
              > $out/nix-support/hydra-build-products

            runHook postInstall
          '';
        in
        {
          packages = rec {
            heytea-api = rustBinary { package = "heytea-api"; };
            heytea-poller = rustBinary { package = "heytea-poller"; };
            heytea-mcp = rustBinary { package = "heytea-mcp"; };
            heytea-cli = rustBinary { package = "heytea-cli"; };
            heytea-site-assets = siteAssets;
            heytea-site = site;
            heytea-migrations = migrations;
            heytea = heytea-cli;
            heytea-assets = assets;
            default = heytea-cli;
          } // pkgs.lib.optionalAttrs (system == serverSystem) {
            nixos-do-image = digitalOceanQcow2Image;
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              pkg-config
              openssl
              openssh
              curl
              dnsutils
              opentofu
              doctl
              cloudflared
              flarectl
              wrangler
              tailscale
              age
              nixos-anywhere
              sqlx-cli
              postgresql_16
              nodejs_22
              pnpm
              jq
              gh
              atkinson-hyperlegible
              dashboardFontTools
              deploy-rs.packages.${system}.deploy-rs
              agenix.packages.${system}.default
            ];

            shellHook = ''
              export SQLX_OFFLINE=true
              export ATKINSON_FONT=${pkgs.atkinson-hyperlegible}/share/fonts/opentype/AtkinsonHyperlegible-Regular.otf
            '';
          };

          formatter = pkgs.nixpkgs-fmt;

          apps.deploy = {
            type = "app";
            program = "${pkgs.writeShellScript "deploy-heytea" ''
              if [ "$#" -eq 0 ] || [ "''${1#-}" != "$1" ]; then
                exec ${deploy-rs.packages.${system}.deploy-rs}/bin/deploy \
                  --auto-rollback true \
                  --magic-rollback true \
                  path:.# \
                  "$@"
              fi

              exec ${deploy-rs.packages.${system}.deploy-rs}/bin/deploy \
                --auto-rollback true \
                --magic-rollback true \
                "$@"
            ''}";
            meta.description = "Deploy heytea.dev with deploy-rs rollback protection";
          };
        }) // {
      nixosModules.heytea = import ./nix/modules/heytea.nix;

      nixosConfigurations.heytea-bootstrap = nixpkgs.lib.nixosSystem {
        system = serverSystem;
        modules = [
          ./nix/hosts/bootstrap.nix
        ];
      };

      nixosConfigurations.heytea-install = nixpkgs.lib.nixosSystem {
        system = serverSystem;
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/install.nix
        ];
      };

      nixosConfigurations.heytea-dev = nixpkgs.lib.nixosSystem {
        system = serverSystem;
        modules = [
          agenix.nixosModules.default
          self.nixosModules.heytea
          ./nix/hosts/heytea.nix
          ({ ... }: {
            services.heytea = {
              apiPackage = self.packages.${serverSystem}.heytea-api;
              pollerPackage = self.packages.${serverSystem}.heytea-poller;
              mcpPackage = self.packages.${serverSystem}.heytea-mcp;
              sitePackage = self.packages.${serverSystem}.heytea-site;
              migrationsPackage = self.packages.${serverSystem}.heytea-migrations;
            };
          })
        ];
      };

      nixosConfigurations.heytea = self.nixosConfigurations.heytea-dev;

      deploy.nodes."heytea-dev" = {
        hostname = "heytea-dev";
        sshUser = "root";
        profiles.system = {
          user = "root";
          path = deploy-rs.lib.x86_64-linux.activate.nixos self.nixosConfigurations.heytea-dev;
        };
      };
    };
}
