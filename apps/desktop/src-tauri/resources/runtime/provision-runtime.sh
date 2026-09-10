#!/usr/bin/env bash
set -euo pipefail

if [ -f /var/lib/unibox-maintenance/pending.json ]; then
  /opt/unibox/synapse-venv/bin/python /opt/unibox/bin/unibox-maintenance.py recover
fi

exec 9>/var/lock/unibox-maintenance.lock
flock -n 9 || { printf 'Another local maintenance operation is running.\n' >&2; exit 1; }

export DEBIAN_FRONTEND=noninteractive
VENV=/opt/unibox/synapse-venv
ETC=/etc/unibox
STATE=/var/lib/unibox
MEDIA=$STATE/media
LOG=/var/log/unibox
SYNAPSE_VERSION=1.160.0

apt-get update -qq
apt-get install -y --no-install-recommends \
  ca-certificates curl jq git openssl postgresql postgresql-contrib \
  python3 python3-venv python3-dev build-essential pkg-config libpq-dev \
  libffi-dev libssl-dev libjpeg-dev libxml2-dev libxslt1-dev zlib1g-dev \
  rustc cargo

if ! id unibox >/dev/null 2>&1; then
  useradd --system --create-home --home-dir /var/lib/unibox --shell /usr/sbin/nologin unibox
fi

install -d -m 0750 -o unibox -g unibox "$STATE" "$MEDIA" "$LOG" /opt/unibox /opt/unibox/bin /opt/unibox/connectors
install -d -m 0755 "$ETC" "$ETC/appservices"

if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
fi
"$VENV/bin/pip" install --disable-pip-version-check --upgrade pip wheel setuptools >/dev/null
"$VENV/bin/pip" install --disable-pip-version-check "matrix-synapse[postgres]==$SYNAPSE_VERSION" >/dev/null

systemctl enable postgresql >/dev/null
systemctl start postgresql

if ! runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='unibox'" | grep -q 1; then
  runuser -u postgres -- createuser unibox
fi
if ! runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_database WHERE datname='synapse'" | grep -q 1; then
  runuser -u postgres -- createdb -O unibox --encoding=UTF8 --locale=C --template=template0 synapse
fi

if [ ! -f "$ETC/unibox.local.signing.key" ]; then
  pushd "$ETC" >/dev/null
  "$VENV/bin/python" -m synapse.app.homeserver \
    --server-name unibox.local \
    --config-path "$ETC/generated.yaml" \
    --generate-config \
    --report-stats=no >/dev/null
  popd >/dev/null
  rm -f "$ETC/generated.yaml" "$ETC/generated.log.config"
fi

if [ ! -f "$ETC/registration.secret" ]; then
  umask 077
  openssl rand -hex 32 > "$ETC/registration.secret"
fi
REGISTRATION_SECRET=$(cat "$ETC/registration.secret")

cat > "$ETC/log.config" <<'EOF'
version: 1
formatters:
  precise:
    format: '%(asctime)s - %(name)s - %(lineno)d - %(levelname)s - %(message)s'
handlers:
  file:
    class: logging.handlers.RotatingFileHandler
    formatter: precise
    filename: /var/log/unibox/synapse.log
    maxBytes: 10485760
    backupCount: 3
loggers:
  synapse.storage.SQL:
    level: INFO
root:
  level: INFO
  handlers: [file]
disable_existing_loggers: false
EOF

if [ ! -s "$ETC/homeserver.yaml" ]; then
cat > "$ETC/homeserver.yaml" <<EOF
server_name: "unibox.local"
public_baseurl: "http://127.0.0.1:8008/"
pid_file: "$STATE/synapse.pid"
web_client_location: null

listeners:
  - port: 8008
    tls: false
    type: http
    bind_addresses: ['127.0.0.1']
    x_forwarded: false
    resources:
      - names: [client]
        compress: false

database:
  name: psycopg2
  args:
    database: synapse
    host: /var/run/postgresql
    cp_min: 1
    cp_max: 5

log_config: "$ETC/log.config"
media_store_path: "$MEDIA"
signing_key_path: "$ETC/unibox.local.signing.key"
registration_shared_secret: "$REGISTRATION_SECRET"
enable_registration: false
enable_registration_without_verification: false
report_stats: false

# Unibox is a local-only homeserver. Federation endpoints are not exposed.
allow_public_rooms_without_auth: false
allow_public_rooms_over_federation: false
trusted_key_servers: []
suppress_key_server_warning: true

app_service_config_files: []
EOF
fi

chown -R unibox:unibox "$STATE" "$LOG"
chown root:unibox "$ETC/unibox.local.signing.key" "$ETC/homeserver.yaml" "$ETC/log.config"
chmod 0640 "$ETC/unibox.local.signing.key" "$ETC/homeserver.yaml"
chmod 0644 "$ETC/log.config"
chmod 0600 "$ETC/registration.secret"

cat > /etc/systemd/system/unibox-synapse.service <<EOF
[Unit]
Description=Unibox local Matrix homeserver
After=network.target postgresql.service
Requires=postgresql.service
StartLimitIntervalSec=120
StartLimitBurst=3

[Service]
Type=simple
User=unibox
Group=unibox
WorkingDirectory=$STATE
ExecStart=$VENV/bin/python -m synapse.app.homeserver -c $ETC/homeserver.yaml
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=$STATE $LOG

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now unibox-synapse >/dev/null

for _ in $(seq 1 60); do
  if curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null

if [ -s "$STATE/matrix.json" ]; then
  # Refuse to overwrite session data. Recovery must preserve existing account history.
  jq -e '.homeserver == "http://127.0.0.1:8008" and (.user_id | type == "string") and (.access_token | type == "string" and length > 0)' "$STATE/matrix.json" >/dev/null || {
    printf 'Stored local session is invalid; restore a backup before continuing.\n' >&2
    exit 1
  }
else
  umask 077
  if [ ! -s "$ETC/local-account.password" ]; then
    openssl rand -base64 36 | tr -d '\n' > "$ETC/local-account.password"
    chmod 0600 "$ETC/local-account.password"
  fi
  PASSWORD=$(cat "$ETC/local-account.password")
  LOGIN=$(jq -nc --arg pass "$PASSWORD" '{type:"m.login.password",identifier:{type:"m.id.user",user:"unibox"},password:$pass,initial_device_display_name:"Unibox Desktop"}')
  if ! RESPONSE=$(curl -fsS -H 'Content-Type: application/json' -d "$LOGIN" http://127.0.0.1:8008/_matrix/client/v3/login 2>/dev/null); then
    "$VENV/bin/register_new_matrix_user" \
      -c "$ETC/homeserver.yaml" \
      -u unibox \
      -p "$PASSWORD" \
      --no-admin \
      http://127.0.0.1:8008 >/dev/null
    RESPONSE=$(curl -fsS -H 'Content-Type: application/json' -d "$LOGIN" http://127.0.0.1:8008/_matrix/client/v3/login)
  fi
  ACCESS_TOKEN=$(printf '%s' "$RESPONSE" | jq -er '.access_token')
  USER_ID=$(printf '%s' "$RESPONSE" | jq -er '.user_id')
  DEVICE_ID=$(printf '%s' "$RESPONSE" | jq -er '.device_id')

  umask 077
  jq -n \
    --arg homeserver 'http://127.0.0.1:8008' \
    --arg user_id "$USER_ID" \
    --arg access_token "$ACCESS_TOKEN" \
    --arg device_id "$DEVICE_ID" \
    '{homeserver:$homeserver,user_id:$user_id,access_token:$access_token,device_id:$device_id}' \
    > "$STATE/matrix.json.tmp"
  mv "$STATE/matrix.json.tmp" "$STATE/matrix.json"
  chown unibox:unibox "$STATE/matrix.json"
  chmod 0600 "$STATE/matrix.json"
fi

# The shared registration secret is only needed to create the local account.
# Remove it from the live Synapse configuration after bootstrap so connectors
# cannot use it to create additional local accounts.
"$VENV/bin/python" - <<'PY'
from pathlib import Path
import yaml
path = Path('/etc/unibox/homeserver.yaml')
cfg = yaml.safe_load(path.read_text())
cfg.pop('registration_shared_secret', None)
path.write_text(yaml.safe_dump(cfg, sort_keys=False))
PY
chown root:unibox "$ETC/homeserver.yaml"
chmod 0640 "$ETC/homeserver.yaml"
systemctl restart unibox-synapse
for _ in $(seq 1 30); do
  curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null 2>&1 && break
  sleep 1
done
curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null

cat > /opt/unibox/bin/unibox-appservice-register <<'PY'
#!/opt/unibox/synapse-venv/bin/python
import pathlib
import sys
import yaml

if len(sys.argv) != 2:
    raise SystemExit('usage: unibox-appservice-register /path/to/registration.yaml')
registration = str(pathlib.Path(sys.argv[1]).resolve())
config_path = pathlib.Path('/etc/unibox/homeserver.yaml')
config = yaml.safe_load(config_path.read_text())
items = list(config.get('app_service_config_files') or [])
if registration not in items:
    items.append(registration)
config['app_service_config_files'] = sorted(set(items))
config_path.write_text(yaml.safe_dump(config, sort_keys=False))
PY
chmod 0755 /opt/unibox/bin/unibox-appservice-register

cat > /opt/unibox/bin/unibox-appservice-unregister <<'PY'
#!/opt/unibox/synapse-venv/bin/python
import pathlib
import sys
import yaml

if len(sys.argv) != 2:
    raise SystemExit('usage: unibox-appservice-unregister /path/to/registration.yaml')
registration = str(pathlib.Path(sys.argv[1]).resolve())
config_path = pathlib.Path('/etc/unibox/homeserver.yaml')
config = yaml.safe_load(config_path.read_text())
config['app_service_config_files'] = [p for p in (config.get('app_service_config_files') or []) if p != registration]
config_path.write_text(yaml.safe_dump(config, sort_keys=False))
PY
chmod 0755 /opt/unibox/bin/unibox-appservice-unregister

OS_PRETTY=$(awk -F= '$1=="PRETTY_NAME" {gsub(/^"|"$/, "", $2); print $2}' /etc/os-release)
POSTGRES_VERSION=$(psql --version | awk '{print $3}')
umask 077
jq -n \
  --arg synapse "$SYNAPSE_VERSION" \
  --arg postgres "$POSTGRES_VERSION" \
  --arg os "$OS_PRETTY" \
  '{synapse:$synapse,postgres:$postgres,os:$os}' > "$STATE/runtime-version.json"
chown unibox:unibox "$STATE/runtime-version.json"
chmod 0600 "$STATE/runtime-version.json"

printf 'Unibox local runtime provisioned successfully.\n'
