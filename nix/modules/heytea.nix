{ config, pkgs, lib, ... }:

let
  cfg = config.services.heytea;
  inherit (lib) mkEnableOption mkIf mkOption types;
  cloudflareCidrs = [
    "173.245.48.0/20"
    "103.21.244.0/22"
    "103.22.200.0/22"
    "103.31.4.0/22"
    "141.101.64.0/18"
    "108.162.192.0/18"
    "190.93.240.0/20"
    "188.114.96.0/20"
    "197.234.240.0/22"
    "198.41.128.0/17"
    "162.158.0.0/15"
    "104.16.0.0/13"
    "104.24.0.0/14"
    "172.64.0.0/13"
    "131.0.72.0/22"
    "2400:cb00::/32"
    "2606:4700::/32"
    "2803:f800::/32"
    "2405:b500::/32"
    "2405:8100::/32"
    "2a06:98c0::/29"
    "2c0f:f248::/32"
  ];
  cloudflareTrustedProxies = lib.concatStringsSep " " cloudflareCidrs;
  sitePreamble = ''
    encode zstd gzip
    tls {
      issuer internal
      protocols tls1.2 tls1.3
    }
  '';
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
    apiPackage = mkOption { type = types.package; default = placeholder "heytea-api"; };
    pollerPackage = mkOption { type = types.package; default = placeholder "heytea-poller"; };
    mcpPackage = mkOption { type = types.package; default = placeholder "heytea-mcp"; };
    sshTuiPackage = mkOption { type = types.package; default = placeholder "heytea-ssh-tui"; };
    sitePackage = mkOption { type = types.package; default = placeholder "heytea-site"; };
    migrationsPackage = mkOption { type = types.package; default = migrationsPlaceholder; };
    geoIpDatabase = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Optional local MaxMind-compatible City MMDB used for site IP geolocation.";
    };
    geoIpAttributionName = mkOption { type = types.str; default = "DB-IP"; };
    geoIpAttributionUrl = mkOption { type = types.str; default = "https://db-ip.com"; };
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
      settings = {
        shared_preload_libraries = "timescaledb";
        max_connections = lib.mkDefault 16;
        shared_buffers = lib.mkDefault "64MB";
        effective_cache_size = lib.mkDefault "256MB";
        maintenance_work_mem = lib.mkDefault "32MB";
        work_mem = lib.mkDefault "1MB";
        wal_buffers = lib.mkDefault "4MB";
        autovacuum_max_workers = lib.mkDefault 1;
        max_worker_processes = lib.mkDefault 4;
        max_parallel_workers = lib.mkDefault 0;
        max_parallel_workers_per_gather = lib.mkDefault 0;
        "timescaledb.max_background_workers" = lib.mkDefault 2;
      };
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
        HEYTEA_API_MAX_CONNECTIONS = "4";
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
        HEYTEA_POLLER_MAX_CONNECTIONS = "2";
        HEYTEA_UPSTREAM_CONCURRENCY = "6";
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
        HEYTEA_MCP_URL = "http://127.0.0.1:3001";
        HEYTEA_PUBLIC_API_URL = "https://${cfg.apiDomain}";
      } // lib.optionalAttrs (cfg.geoIpDatabase != null) {
        HEYTEA_GEOIP_MMDB = toString cfg.geoIpDatabase;
        HEYTEA_GEOIP_ATTRIBUTION_NAME = cfg.geoIpAttributionName;
        HEYTEA_GEOIP_ATTRIBUTION_URL = cfg.geoIpAttributionUrl;
      };
    };

    systemd.services.heytea-ssh-tui = {
      wantedBy = [ "multi-user.target" ];
      wants = [ "network-online.target" ];
      after = [ "network-online.target" "heytea-api.service" ];
      preStart = ''
        key=/var/lib/heytea-ssh-tui/ssh_host_ed25519_key
        if [ ! -s "$key" ]; then
          rm -f "$key" "$key.pub"
          ${pkgs.openssh}/bin/ssh-keygen -q -t ed25519 -N "" -f "$key"
        fi
        ${pkgs.coreutils}/bin/chmod 0600 "$key"
      '';
      serviceConfig = {
        ExecStart = "${cfg.sshTuiPackage}/bin/heytea-ssh-tui";
        Restart = "always";
        DynamicUser = true;
        StateDirectory = "heytea-ssh-tui";
        AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
        CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" ];
        LimitNOFILE = 256;
        TasksMax = 64;
        MemoryMax = "96M";
      };
      environment = {
        HEYTEA_API_URL = "http://127.0.0.1:3000";
        HEYTEA_SSH_TUI_BIND = "[::]:22";
        HEYTEA_SSH_TUI_HOST_KEY = "/var/lib/heytea-ssh-tui/ssh_host_ed25519_key";
        HEYTEA_SSH_TUI_MAX_SESSIONS = "8";
        HEYTEA_SSH_TUI_REFRESH_SECONDS = "15";
        RUST_LOG = "info";
      };
    };

    services.caddy = {
      enable = true;
      globalConfig = ''
        servers {
          protocols h1 h2 h3
          trusted_proxies static ${cloudflareTrustedProxies}
          trusted_proxies_strict
        }
      '';
      virtualHosts = {
        ${cfg.domain}.extraConfig = ''
          ${sitePreamble}
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
        ${cfg.apiDomain}.extraConfig = ''
          ${sitePreamble}
          reverse_proxy 127.0.0.1:3000
        '';
        ${cfg.docsDomain}.extraConfig = ''
          ${sitePreamble}
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
        ${cfg.statusDomain}.extraConfig = ''
          ${sitePreamble}
          handle / {
            rewrite * /status
            reverse_proxy 127.0.0.1:3100
          }
          handle {
            reverse_proxy 127.0.0.1:3100
          }
        '';
        ${cfg.mcpDomain}.extraConfig = ''
          ${sitePreamble}
          reverse_proxy 127.0.0.1:3001
        '';
      };
    };
  };
}
