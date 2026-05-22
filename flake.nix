{
  description = "heytea.dev location finder and wait-time platform";

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
          version = "0.1.1";

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

          dbipCityLiteMmdb = pkgs.runCommand "dbip-city-lite-mmdb-2026-05"
            {
              nativeBuildInputs = [ pkgs.gzip ];
              src = pkgs.fetchurl {
                url = "https://download.db-ip.com/free/dbip-city-lite-2026-05.mmdb.gz";
                hash = "sha256-lSDMjGXcBMr8iGgqyJ26LPu8tfaG6QlKsCH88q4sAvA=";
              };
            }
            ''
              runHook preInstall
              mkdir -p $out/share/heytea
              gzip -dc $src > $out/share/heytea/dbip-city-lite.mmdb
              runHook postInstall
            '';

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

          heyteaAdmin = pkgs.writeShellScriptBin "heytea-admin" ''
            set -euo pipefail

            psql=${pkgs.postgresql_16}/bin/psql
            db_url="''${DATABASE_URL:-postgresql:///heytea?host=/run/postgresql&user=heytea}"

            usage() {
              printf '%s\n' \
                'usage: heytea-admin <command> [args]' \
                "" \
                'commands:' \
                '  managed list              list active managed locations' \
                '  managed list-all          list every location with managed state' \
                '  managed add <slug> [note] add or reactivate a managed override' \
                '  managed remove <slug>     force a location unmanaged' \
                '  managed recompute         recompute region/manual managed state' \
                '  managed regions           list managed seed regions' >&2
            }

            run_psql() {
              "$psql" -v ON_ERROR_STOP=1 -P pager=off "$db_url" "$@"
            }

            require_slug() {
              if [ "$#" -lt 1 ] || [ -z "''${1:-}" ]; then
                usage
                exit 2
              fi
            }

            if [ "$#" -lt 1 ]; then
              usage
              exit 2
            fi

            scope="$1"
            shift
            if [ "$scope" != managed ]; then
              usage
              exit 2
            fi

            command="''${1:-list}"
            if [ "$#" -gt 0 ]; then
              shift
            fi

            case "$command" in
              list)
                run_psql -c "select refresh_managed_locations();" >/dev/null
                run_psql -c "select l.slug, l.name, l.address, ml.source, ml.reason from managed_locations ml join locations l on l.shop_id = ml.shop_id where ml.is_active is true order by l.name;"
                ;;
              list-all)
                run_psql -c "select refresh_managed_locations();" >/dev/null
                run_psql -c "select l.slug, l.name, case when ml.is_active is true then 'managed' else 'unmanaged' end as state, coalesce(ml.source, '-') as source, coalesce(ml.reason, '-') as reason from locations l left join managed_locations ml on ml.shop_id = l.shop_id where coalesce(l.is_enabled, true) is true order by state, l.name;"
                ;;
              add)
                require_slug "$@"
                slug="$1"
                note="''${2:-manual override}"
                "$psql" -v ON_ERROR_STOP=1 -P pager=off -v slug="$slug" -v note="$note" "$db_url" <<'SQL'
            with target as (
              select shop_id from locations where slug = :'slug'
            )
            insert into managed_location_overrides (shop_id, is_managed, note, updated_at)
            select shop_id, true, :'note', now()
            from target
            on conflict (shop_id) do update set
              is_managed = true,
              note = excluded.note,
              updated_at = now();

            select refresh_managed_locations();
SQL
                ;;
              remove)
                require_slug "$@"
                slug="$1"
                "$psql" -v ON_ERROR_STOP=1 -P pager=off -v slug="$slug" "$db_url" <<'SQL'
            with target as (
              select shop_id from locations where slug = :'slug'
            )
            insert into managed_location_overrides (shop_id, is_managed, note, updated_at)
            select shop_id, false, 'manual unmanaged override', now()
            from target
            on conflict (shop_id) do update set
              is_managed = false,
              note = excluded.note,
              updated_at = now();

            select refresh_managed_locations();
SQL
                ;;
              recompute)
                run_psql -c "select refresh_managed_locations();"
                ;;
              regions)
                run_psql -c "select slug, label, latitude, longitude, radius_miles, is_enabled from managed_regions order by slug;"
                ;;
              *)
                usage
                exit 2
                ;;
            esac
          '';

        in
        {
          packages = rec {
            heytea-api = rustBinary { package = "heytea-api"; };
            heytea-poller = rustBinary { package = "heytea-poller"; };
            heytea-mcp = rustBinary { package = "heytea-mcp"; };
            heytea-ssh-tui = rustBinary { package = "heytea-ssh-tui"; };
            heytea-cli = rustBinary { package = "heytea-cli"; };
            heytea-site-assets = siteAssets;
            heytea-site = site;
            heytea-admin = heyteaAdmin;
            heytea-migrations = migrations;
            dbip-city-lite-mmdb = dbipCityLiteMmdb;
            heytea = heytea-cli;
            heytea-assets = assets;
            default = heytea-cli;
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
                  path:.#heytea-dev \
                  "$@"
              fi

              exec ${deploy-rs.packages.${system}.deploy-rs}/bin/deploy \
                --auto-rollback true \
                --magic-rollback true \
                "$@"
            ''}";
            meta.description = "Deploy heytea.dev with deploy-rs rollback protection";
          };

          apps.admin = {
            type = "app";
            program = "${heyteaAdmin}/bin/heytea-admin";
            meta.description = "Administer heytea.dev managed tracking state";
          };

          apps.managed = {
            type = "app";
            program = "${heyteaAdmin}/bin/heytea-admin";
            meta.description = "Alias for heytea-admin managed tracking commands";
          };
        }) // {
      nixosModules.heytea = import ./nix/modules/heytea.nix;

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
              sshTuiPackage = self.packages.${serverSystem}.heytea-ssh-tui;
              sitePackage = self.packages.${serverSystem}.heytea-site;
              migrationsPackage = self.packages.${serverSystem}.heytea-migrations;
              geoIpDatabase = "${self.packages.${serverSystem}.dbip-city-lite-mmdb}/share/heytea/dbip-city-lite.mmdb";
            };
          })
        ];
      };

      nixosConfigurations.heytea-dev-bootstrap = nixpkgs.lib.nixosSystem {
        system = serverSystem;
        modules = [
          agenix.nixosModules.default
          self.nixosModules.heytea
          ./nix/hosts/heytea.nix
          ({ lib, ... }: {
            services.openssh.ports = lib.mkForce [ 22 ];
            systemd.services.heytea-ssh-tui.wantedBy = lib.mkForce [ ];
            systemd.services.tailscale-autoconnect.wantedBy = lib.mkForce [ ];
          })
          ({ ... }: {
            services.heytea = {
              apiPackage = self.packages.${serverSystem}.heytea-api;
              pollerPackage = self.packages.${serverSystem}.heytea-poller;
              mcpPackage = self.packages.${serverSystem}.heytea-mcp;
              sshTuiPackage = self.packages.${serverSystem}.heytea-ssh-tui;
              sitePackage = self.packages.${serverSystem}.heytea-site;
              migrationsPackage = self.packages.${serverSystem}.heytea-migrations;
              geoIpDatabase = "${self.packages.${serverSystem}.dbip-city-lite-mmdb}/share/heytea/dbip-city-lite.mmdb";
            };
          })
        ];
      };

      nixosConfigurations.heytea = self.nixosConfigurations.heytea-dev;

      deploy.nodes."heytea-dev" = {
        hostname = "heytea-dev-2";
        sshUser = "root";
        sshOpts = [ "-p" "2222" ];
        profiles.system = {
          user = "root";
          path = deploy-rs.lib.x86_64-linux.activate.nixos self.nixosConfigurations.heytea-dev;
        };
      };

      deploy.nodes."heytea-dev-bootstrap" = {
        hostname = "heytea-dev-bootstrap";
        sshUser = "root";
        sshOpts = [ "-p" "22" ];
        profiles.system = {
          user = "root";
          path = deploy-rs.lib.x86_64-linux.activate.nixos self.nixosConfigurations.heytea-dev-bootstrap;
        };
      };
    };
}
