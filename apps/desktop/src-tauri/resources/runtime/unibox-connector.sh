#!/usr/bin/env bash
set -euo pipefail

ROOT=/opt/unibox/connectors
ETC=/etc/unibox
PY=/opt/unibox/synapse-venv/bin/python

fail() { printf 'Unibox connector error: %s\n' "$*" >&2; exit 1; }
valid_id() { [[ "$1" =~ ^[a-z0-9_-]+$ ]]; }

ensure_db() {
  local id="$1" db="unibox_${1//-/_}"
  if ! runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_database WHERE datname='$db'" | grep -q 1; then
    runuser -u postgres -- createdb -O unibox --encoding=UTF8 --locale=C --template=template0 "$db"
  fi
  printf '%s' "$db"
}

latest_asset() {
  local repo="$1" asset="$2" out="$3"
  local json url digest expected actual
  json=$(curl -fsSL -H 'Accept: application/vnd.github+json' -H 'User-Agent: Unibox' "https://api.github.com/repos/$repo/releases/latest")
  url=$(printf '%s' "$json" | jq -er --arg name "$asset" '.assets[] | select(.name==$name) | .browser_download_url' | head -n1)
  digest=$(printf '%s' "$json" | jq -er --arg name "$asset" '.assets[] | select(.name==$name) | .digest // empty' | head -n1)
  [[ "$digest" == sha256:* ]] || fail "$repo latest release does not publish a SHA-256 digest for $asset"
  expected="${digest#sha256:}"
  curl -fL --retry 3 --retry-delay 2 -H 'User-Agent: Unibox' "$url" -o "$out"
  actual=$(sha256sum "$out" | awk '{print $1}')
  [[ "$actual" == "$expected" ]] || fail "SHA-256 mismatch for $asset"
}

patch_config() {
  local id="$1" port="$2" adapter="$3" config="$4" db="$5"
  "$PY" - "$id" "$port" "$adapter" "$config" "$db" <<'PY'
import pathlib, sys, yaml
cid, port, adapter, path, db = sys.argv[1], int(sys.argv[2]), sys.argv[3], pathlib.Path(sys.argv[4]), sys.argv[5]
cfg = yaml.safe_load(path.read_text()) or {}

hs = cfg.setdefault('homeserver', {})
hs['address'] = 'http://127.0.0.1:8008'
hs['domain'] = 'unibox.local'
hs['software'] = 'standard'

app = cfg.setdefault('appservice', {})
app['address'] = f'http://127.0.0.1:{port}'
app['hostname'] = '127.0.0.1'
app['port'] = port
app['id'] = f'unibox-{cid}'

if adapter == 'bridgev2':
    database = cfg.setdefault('database', {})
    database['type'] = 'postgres'
    database['uri'] = f'postgres:///{db}?host=/var/run/postgresql'
    database.setdefault('max_open_conns', 10)
    database.setdefault('max_idle_conns', 2)

    bridge = cfg.setdefault('bridge', {})
    bridge['permissions'] = {'@unibox:unibox.local': 'admin'}
    matrix = cfg.setdefault('matrix', {})
    matrix['federate_rooms'] = False
    encryption = cfg.setdefault('encryption', {})
    encryption['allow'] = False
    encryption['default'] = False
    provisioning = cfg.setdefault('provisioning', {})
    provisioning['allow_matrix_auth'] = True
    provisioning['shared_secret'] = ''
else:
    database = app.get('database')
    if isinstance(database, dict):
        database['type'] = 'postgres'
        database['uri'] = f'postgres:///{db}?host=/var/run/postgresql'
    else:
        app['database'] = {
            'type': 'postgres',
            'uri': f'postgres:///{db}?host=/var/run/postgresql',
            'max_open_conns': 10,
            'max_idle_conns': 2,
        }
    bridge = cfg.setdefault('bridge', {})
    bridge['permissions'] = {'@unibox:unibox.local': 'admin'}
    bridge['federate_rooms'] = False
    encryption = bridge.setdefault('encryption', {})
    encryption['allow'] = False
    encryption['default'] = False

path.write_text(yaml.safe_dump(cfg, sort_keys=False))
PY
  chown unibox:unibox "$config"
  chmod 0600 "$config"
}

install_service() {
  local id="$1" executable="$2" config="$3" state="$4"
  cat > "/etc/systemd/system/unibox-$id.service" <<EOF
[Unit]
Description=Unibox $id connector
After=network.target unibox-synapse.service postgresql.service
Requires=unibox-synapse.service postgresql.service

[Service]
Type=simple
User=unibox
Group=unibox
WorkingDirectory=$state
ExecStart=$executable -c $config
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=$state /var/log/unibox

[Install]
WantedBy=multi-user.target
EOF
  systemctl daemon-reload
  systemctl enable "unibox-$id.service" >/dev/null
}

register_appservice() {
  local registration="$1"
  local target="$ETC/appservices/$(basename "$registration")"
  install -m 0640 -o root -g unibox "$registration" "$target"
  /opt/unibox/bin/unibox-appservice-register "$target"
  systemctl restart unibox-synapse
  for _ in $(seq 1 30); do
    curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null 2>&1 && return 0
    sleep 1
  done
  fail 'Synapse did not recover after appservice registration'
}

install_go() {
  local id="$1" adapter="$2" repo="$3" binary="$4" asset="$5" port="$6"
  local dir="$ROOT/$id" state="$dir/state" executable="$dir/$binary" tmp db config registration
  install -d -m 0750 -o unibox -g unibox "$dir" "$state"
  tmp=$(mktemp)
  trap 'rm -f "$tmp"' RETURN
  latest_asset "$repo" "$asset" "$tmp"
  install -m 0755 -o root -g root "$tmp" "$executable"
  db=$(ensure_db "$id")
  config="$state/config.yaml"
  registration="$state/registration.yaml"

  if [ ! -f "$config" ]; then
    if [ "$adapter" = 'bridgev2' ]; then
      runuser -u unibox -- "$executable" -e -c "$config"
    else
      curl -fsSL -H 'User-Agent: Unibox' "https://raw.githubusercontent.com/$repo/main/example-config.yaml" -o "$config"
      chown unibox:unibox "$config"
    fi
  fi

  patch_config "$id" "$port" "$adapter" "$config" "$db"
  rm -f "$registration"
  runuser -u unibox -- "$executable" -g -c "$config" -r "$registration"
  register_appservice "$registration"
  install_service "$id" "$executable" "$config" "$state"
  systemctl restart "unibox-$id.service"
  sleep 2
  systemctl is-active --quiet "unibox-$id.service" || {
    journalctl -u "unibox-$id.service" -n 30 --no-pager >&2 || true
    fail "$id failed to start"
  }
}

install_python_googlechat() {
  local id="$1" repo="$2" port="$3" dir="$ROOT/$id" state="$ROOT/$id/state" db config registration venv
  install -d -m 0750 -o unibox -g unibox "$dir" "$state"
  venv="$dir/venv"
  if [ ! -x "$venv/bin/python" ]; then
    python3 -m venv "$venv"
    "$venv/bin/pip" install --disable-pip-version-check --upgrade pip wheel setuptools >/dev/null
    "$venv/bin/pip" install --disable-pip-version-check "git+https://github.com/$repo.git" >/dev/null
  fi
  db=$(ensure_db "$id")
  config="$state/config.yaml"
  registration="$state/registration.yaml"
  if [ ! -f "$config" ]; then
    local example
    example=$("$venv/bin/python" - <<'PY'
import pathlib, mautrix_googlechat
print(pathlib.Path(mautrix_googlechat.__file__).parent / 'example-config.yaml')
PY
)
    cp "$example" "$config"
    chown unibox:unibox "$config"
  fi

  "$PY" - "$config" "$db" "$port" <<'PY'
import pathlib, sys, yaml
path, db, port = pathlib.Path(sys.argv[1]), sys.argv[2], int(sys.argv[3])
cfg = yaml.safe_load(path.read_text()) or {}
cfg.setdefault('homeserver', {}).update({'address':'http://127.0.0.1:8008','domain':'unibox.local','verify_ssl':False,'software':'standard'})
app = cfg.setdefault('appservice', {})
app.update({'address':f'http://127.0.0.1:{port}','hostname':'127.0.0.1','port':port,'database':f'postgres:///{db}?host=/var/run/postgresql'})
bridge = cfg.setdefault('bridge', {})
bridge['permissions'] = {'@unibox:unibox.local':'admin'}
bridge['federate_rooms'] = False
bridge.setdefault('encryption', {}).update({'allow':False,'default':False})
bridge.setdefault('provisioning', {})['shared_secret'] = 'generate'
path.write_text(yaml.safe_dump(cfg, sort_keys=False))
PY
  chown unibox:unibox "$config"; chmod 0600 "$config"
  rm -f "$registration"
  runuser -u unibox -- "$venv/bin/python" -m mautrix_googlechat -g -c "$config" -r "$registration"
  register_appservice "$registration"

  cat > "/etc/systemd/system/unibox-$id.service" <<EOF
[Unit]
Description=Unibox Google Chat connector
After=network.target unibox-synapse.service postgresql.service
Requires=unibox-synapse.service postgresql.service
[Service]
Type=simple
User=unibox
Group=unibox
WorkingDirectory=$state
ExecStart=$venv/bin/python -m mautrix_googlechat -c $config
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=$state /var/log/unibox
[Install]
WantedBy=multi-user.target
EOF
  systemctl daemon-reload
  systemctl enable --now "unibox-$id.service" >/dev/null
  touch "$dir/.installed"; chown unibox:unibox "$dir/.installed"
}

update_go() {
  local id="$1" adapter="$2" repo="$3" binary="$4" asset="$5"
  local dir="$ROOT/$id" executable="$dir/$binary" tmp backup
  [ -x "$executable" ] || fail "$id is not installed"
  tmp=$(mktemp); backup="$executable.rollback"
  trap 'rm -f "$tmp"' RETURN
  latest_asset "$repo" "$asset" "$tmp"
  if cmp -s "$tmp" "$executable"; then
    printf '%s is already up to date.\n' "$id"
    return 0
  fi
  systemctl stop "unibox-$id.service" || true
  cp -a "$executable" "$backup"
  install -m 0755 -o root -g root "$tmp" "$executable"
  if systemctl start "unibox-$id.service" && sleep 2 && systemctl is-active --quiet "unibox-$id.service"; then
    rm -f "$backup"
    printf '%s updated successfully.\n' "$id"
  else
    systemctl stop "unibox-$id.service" || true
    mv -f "$backup" "$executable"
    systemctl start "unibox-$id.service" || true
    fail "$id update failed and was rolled back"
  fi
}

[ "$#" -ge 2 ] || fail 'usage: unibox-connector <install|start|stop|update> <id> ...'
cmd="$1"; id="$2"; shift 2
valid_id "$id" || fail 'invalid connector id'

case "$cmd" in
  install)
    [ "$#" -eq 5 ] || fail 'install requires adapter repo binary asset port'
    adapter="$1"; repo="$2"; binary="$3"; asset="$4"; port="$5"
    if [ "$adapter" = 'python-legacy' ]; then
      [ "$id" = 'googlechat' ] || fail 'unsupported Python legacy connector'
      install_python_googlechat "$id" "$repo" "$port"
    else
      install_go "$id" "$adapter" "$repo" "$binary" "$asset" "$port"
    fi
    ;;
  start) systemctl start "unibox-$id.service" ;;
  stop) systemctl stop "unibox-$id.service" ;;
  update)
    [ "$#" -eq 4 ] || fail 'update requires adapter repo binary asset'
    adapter="$1"; repo="$2"; binary="$3"; asset="$4"
    if [ "$adapter" = 'python-legacy' ]; then
      fail 'Python legacy connector updates are applied by reinstalling the connector in this release'
    fi
    update_go "$id" "$adapter" "$repo" "$binary" "$asset"
    ;;
  *) fail "unknown command: $cmd" ;;
esac
