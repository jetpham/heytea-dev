{ config, pkgs, lib, ... }:

let
  cfg = config.services.heytea;
  inherit (lib) mkEnableOption mkIf mkOption types;
  placeholder = name: pkgs.writeShellScriptBin name ''
    echo "${name} package has not been wired into the Nix build yet" >&2
    exit 1
  '';
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
    frontendRoot = mkOption { type = types.path; default = pkgs.runCommand "heytea-empty-frontend" { } "mkdir -p $out; echo placeholder > $out/index.html"; };
  };

  config = mkIf cfg.enable {
    environment.etc."heytea/shop-id".text = "1000092\n";

    services.postgresql = {
      enable = true;
      package = pkgs.postgresql_16.withPackages (ps: [ ps.timescaledb ]);
      settings.shared_preload_libraries = "timescaledb";
      ensureDatabases = [ "heytea" "umami" ];
      ensureUsers = [
        {
          name = "heytea";
          ensureDBOwnership = true;
        }
        {
          name = "umami";
          ensureDBOwnership = true;
        }
      ];
    };

    services.redis.servers.heytea = {
      enable = true;
      bind = "127.0.0.1";
      port = 6379;
    };

    systemd.services.heytea-api = {
      wantedBy = [ "multi-user.target" ];
      after = [ "postgresql.service" "redis-heytea.service" ];
      serviceConfig = {
        ExecStart = "${cfg.apiPackage}/bin/heytea-api";
        Restart = "always";
        DynamicUser = true;
      };
      environment = {
        DATABASE_URL = "postgres://heytea@/heytea?host=/run/postgresql";
        HEYTEA_API_BIND = "127.0.0.1:3000";
        RUST_LOG = "info";
      };
    };

    systemd.services.heytea-poller = {
      wantedBy = [ "multi-user.target" ];
      after = [ "postgresql.service" ];
      serviceConfig = {
        ExecStart = "${cfg.pollerPackage}/bin/heytea-poller";
        Restart = "always";
        DynamicUser = true;
        ReadOnlyPaths = [ cfg.shopConfigPath ];
      };
      environment = {
        DATABASE_URL = "postgres://heytea@/heytea?host=/run/postgresql";
        HEYTEA_SHOP_CONFIG = toString cfg.shopConfigPath;
        HEYTEA_POLLER_INTERVAL_SECONDS = "60";
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

    services.caddy = {
      enable = true;
      virtualHosts.${cfg.domain}.extraConfig = ''
        root * ${cfg.frontendRoot}
        file_server
      '';
      virtualHosts.${cfg.apiDomain}.extraConfig = ''
        reverse_proxy 127.0.0.1:3000
      '';
      virtualHosts.${cfg.docsDomain}.extraConfig = ''
        reverse_proxy 127.0.0.1:3000
      '';
      virtualHosts.${cfg.mcpDomain}.extraConfig = ''
        reverse_proxy 127.0.0.1:3001
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
