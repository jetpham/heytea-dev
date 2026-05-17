{ config, pkgs, lib, ... }:

let
  cfg = config.services.heytea;
  inherit (lib) mkEnableOption mkIf mkOption types;
  placeholder = name: pkgs.writeShellScriptBin name ''
    echo "${name} package has not been wired into the Nix build yet" >&2
    exit 1
  '';
  migrationsPlaceholder = pkgs.runCommand "heytea-migrations-unconfigured" { } ''
    mkdir -p $out/share/heytea/migrations
  '';
  migrationsDir = "${cfg.migrationsPackage}/share/heytea/migrations";
in
{
  options.services.heytea = {
    enable = mkEnableOption "heytea.dev services";
    domain = mkOption { type = types.str; default = "heytea.dev"; };
    apiDomain = mkOption { type = types.str; default = "api.heytea.dev"; };
    docsDomain = mkOption { type = types.str; default = "docs.heytea.dev"; };
    mcpDomain = mkOption { type = types.str; default = "mcp.heytea.dev"; };
    statusDomain = mkOption { type = types.str; default = "status.heytea.dev"; };
    analyticsDomain = mkOption { type = types.str; default = "analytics.heytea.dev"; };
    shopConfigPath = mkOption { type = types.str; default = "/etc/heytea/shop-id"; };
    apiPackage = mkOption { type = types.package; default = placeholder "heytea-api"; };
    pollerPackage = mkOption { type = types.package; default = placeholder "heytea-poller"; };
    mcpPackage = mkOption { type = types.package; default = placeholder "heytea-mcp"; };
    sitePackage = mkOption { type = types.package; default = placeholder "heytea-site"; };
    migrationsPackage = mkOption { type = types.package; default = migrationsPlaceholder; };
  };

  config = mkIf cfg.enable {
    users.groups.heytea = { };
    users.users.heytea = {
      isSystemUser = true;
      group = "heytea";
    };

    services.postgresql = {
      enable = true;
      package = pkgs.postgresql_16.withPackages (ps: [ ps.timescaledb ]);
      settings.shared_preload_libraries = "timescaledb";
      authentication = lib.mkForce ''
        local all all peer
        host all all 127.0.0.1/32 reject
        host all all ::1/128 reject
      '';
      ensureDatabases = [ "heytea" ];
      ensureUsers = [
        {
          name = "heytea";
          ensureDBOwnership = true;
        }
      ];
    };

    systemd.services.heytea-db-migrate = {
      wantedBy = [ "multi-user.target" ];
      requires = [ "postgresql.service" "postgresql-setup.service" ];
      after = [ "postgresql.service" "postgresql-setup.service" ];
      before = [ "heytea-api.service" "heytea-poller.service" ];
      path = [ config.services.postgresql.package ];
      serviceConfig = {
        Type = "oneshot";
        User = "postgres";
        Group = "postgres";
        RemainAfterExit = true;
      };
      script = ''
        found=0
        for migration in ${migrationsDir}/*.sql; do
          if [ ! -e "$migration" ]; then
            continue
          fi
          found=1
          echo "applying $migration"
          psql -v ON_ERROR_STOP=1 --dbname=heytea --file="$migration"
        done
        if [ "$found" -eq 0 ]; then
          echo "no heytea migrations found in ${migrationsDir}" >&2
          exit 1
        fi
      '';
    };

    systemd.services.heytea-api = {
      wantedBy = [ "multi-user.target" ];
      requires = [ "heytea-db-migrate.service" ];
      after = [ "postgresql.service" "heytea-db-migrate.service" ];
      serviceConfig = {
        ExecStart = "${cfg.apiPackage}/bin/heytea-api";
        Restart = "always";
        User = "heytea";
        Group = "heytea";
      };
      environment = {
        DATABASE_URL = "postgresql:///heytea?host=/run/postgresql&user=heytea";
        HEYTEA_API_BIND = "127.0.0.1:3000";
        RUST_LOG = "info";
      };
    };

    systemd.services.heytea-poller = {
      wantedBy = [ "multi-user.target" ];
      requires = [ "heytea-db-migrate.service" ];
      after = [ "postgresql.service" "heytea-db-migrate.service" ];
      serviceConfig = {
        ExecStart = "${cfg.pollerPackage}/bin/heytea-poller";
        Restart = "always";
        User = "heytea";
        Group = "heytea";
      };
      environment = {
        DATABASE_URL = "postgresql:///heytea?host=/run/postgresql&user=heytea";
        HEYTEA_POLLER_INTERVAL_SECONDS = "60";
        HEYTEA_CATALOG_INTERVAL_SECONDS = "86400";
        HEYTEA_UPSTREAM_CONCURRENCY = "16";
        RUST_LOG = "info";
      };
    };

    systemd.services.heytea-mcp = {
      wantedBy = [ "multi-user.target" ];
      after = [ "heytea-api.service" ];
      serviceConfig = {
        ExecStart = "${cfg.mcpPackage}/bin/heytea-mcp";
        Restart = "always";
        DynamicUser = true;
      };
      environment = {
        HEYTEA_API_URL = "http://127.0.0.1:3000";
        HEYTEA_MCP_BIND = "127.0.0.1:3001";
        RUST_LOG = "info";
      };
    };

    systemd.services.heytea-site = {
      wantedBy = [ "multi-user.target" ];
      after = [ "heytea-api.service" ];
      serviceConfig = {
        ExecStart = "${cfg.sitePackage}/bin/heytea-site";
        Restart = "always";
        DynamicUser = true;
      };
      environment = {
        HEYTEA_SITE_BIND = "127.0.0.1:3100";
        HEYTEA_API_URL = "http://127.0.0.1:3000";
        HEYTEA_PUBLIC_API_URL = "https://${cfg.apiDomain}";
        HEYTEA_PROMETHEUS_URL = "http://127.0.0.1:9090";
      };
    };

    services.caddy = {
      enable = true;
      virtualHosts.${cfg.domain}.extraConfig = ''
        encode {
          zstd best
          gzip 9
        }
        handle /openapi.json {
          reverse_proxy 127.0.0.1:3000
        }
        handle /mcp* {
          reverse_proxy 127.0.0.1:3001
        }
        handle /assets/* {
          header Cache-Control "public, max-age=31536000, immutable"
          reverse_proxy 127.0.0.1:3100
        }
        handle /a.woff2 {
          header Cache-Control "public, max-age=31536000, immutable"
          reverse_proxy 127.0.0.1:3100
        }
        handle {
          reverse_proxy 127.0.0.1:3100
        }
      '';
      virtualHosts.${cfg.apiDomain}.extraConfig = ''
        encode {
          zstd best
          gzip 9
        }
        reverse_proxy 127.0.0.1:3000
      '';
      virtualHosts.${cfg.docsDomain}.extraConfig = ''
        encode {
          zstd best
          gzip 9
        }
        handle /openapi.json {
          reverse_proxy 127.0.0.1:3000
        }
        handle / {
          rewrite * /docs
          reverse_proxy 127.0.0.1:3100
        }
        handle {
          reverse_proxy 127.0.0.1:3100
        }
      '';
      virtualHosts.${cfg.mcpDomain}.extraConfig = ''
        encode {
          zstd best
          gzip 9
        }
        reverse_proxy 127.0.0.1:3001
      '';
      virtualHosts.${cfg.statusDomain}.extraConfig = ''
        encode {
          zstd best
          gzip 9
        }
        handle / {
          rewrite * /status
          reverse_proxy 127.0.0.1:3100
        }
        handle {
          reverse_proxy 127.0.0.1:3100
        }
      '';
      virtualHosts.${cfg.analyticsDomain}.extraConfig = ''
        encode {
          zstd best
          gzip 9
        }
        reverse_proxy 127.0.0.1:3003
      '';
    };

    services.prometheus = {
      enable = true;
      port = 9090;
      exporters.node.enable = true;
      scrapeConfigs = [
        {
          job_name = "heytea-api";
          static_configs = [{ targets = [ "127.0.0.1:3000" ]; }];
          metrics_path = "/metrics";
        }
        {
          job_name = "blackbox-public";
          metrics_path = "/probe";
          params.module = [ "http_2xx" ];
          static_configs = [{
            targets = [
              "https://${cfg.domain}"
              "https://${cfg.apiDomain}/healthz"
              "https://${cfg.apiDomain}/readyz"
              "https://${cfg.docsDomain}"
              "https://${cfg.mcpDomain}"
              "https://${cfg.statusDomain}"
            ];
          }];
          relabel_configs = [
            { source_labels = [ "__address__" ]; target_label = "__param_target"; }
            { source_labels = [ "__param_target" ]; target_label = "instance"; }
            { target_label = "__address__"; replacement = "127.0.0.1:9115"; }
          ];
        }
      ];
    };

    services.prometheus.exporters.blackbox = {
      enable = true;
      port = 9115;
      configFile = pkgs.writeText "blackbox.yml" ''
        modules:
          http_2xx:
            prober: http
            timeout: 5s
            http:
              valid_http_versions: ["HTTP/1.1", "HTTP/2.0"]
              follow_redirects: true
      '';
    };

    services.grafana = {
      enable = true;
      settings.server = {
        http_addr = "127.0.0.1";
        http_port = 3002;
        domain = "grafana.heytea.dev";
      };
    };
  };
}
