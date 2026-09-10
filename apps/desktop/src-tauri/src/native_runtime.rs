use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{Hmac, Mac};
use rand::{distributions::Alphanumeric, Rng};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_yaml::{Mapping, Value as YamlValue};
use sha1::Sha1;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Read,
    net::{IpAddr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use unibox_core::{
    ClientHttpRequest, ClientHttpResponse, ConnectorDefinition, ConnectorStatus, MatrixSession,
    RuntimeStatus,
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const MATRIX_URL: &str = "http://127.0.0.1:8008";
const MATRIX_USER: &str = "unibox";
const CREATE_NO_WINDOW: u32 = 0x08000000;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NativeSecrets {
    registration_shared_secret: String,
    matrix_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ConnectorLocalSettings {
    #[serde(default)]
    api_id: Option<i64>,
    #[serde(default)]
    api_hash: Option<String>,
}

#[derive(Default)]
struct ChildProcesses {
    homeserver: Option<Child>,
    connectors: HashMap<String, Child>,
}

pub struct NativeRuntimeManager {
    data_root: PathBuf,
    resource_root: PathBuf,
    http: Client,
    remote_http: Client,
    children: Mutex<ChildProcesses>,
}

impl NativeRuntimeManager {
    pub fn new(data_root: impl Into<PathBuf>, resource_root: impl Into<PathBuf>) -> Result<Self> {
        let data_root = data_root.into().join("runtime-native");
        let resource_root = resource_root.into();
        stop_legacy_native_slot_processes(&resource_root);
        fs::create_dir_all(&data_root)
            .with_context(|| format!("failed to create {}", data_root.display()))?;
        Ok(Self {
            data_root,
            resource_root,
            http: Client::builder().timeout(Duration::from_secs(4)).build()?,
            remote_http: Client::builder()
                .timeout(Duration::from_secs(70))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            children: Mutex::new(ChildProcesses::default()),
        })
    }

    pub async fn status(&self) -> RuntimeStatus {
        let ready = self.matrix_ready().await;
        RuntimeStatus {
            platform: std::env::consts::OS.to_string(),
            // Legacy field names are retained temporarily for frontend compatibility.
            // They now describe the bundled native engine rather than WSL.
            wsl_available: self.tuwunel_path().is_file(),
            distro_installed: self.tuwunel_path().is_file(),
            distro_running: ready,
            synapse_ready: ready,
            matrix_session_ready: ready && self.matrix_session().is_ok(),
            data_root: self.data_root.display().to_string(),
        }
    }

    pub async fn bootstrap(&self) -> Result<String> {
        self.prepare_layout()?;
        self.ensure_config()?;
        self.start().await?;
        if self.matrix_session().is_err() {
            let session = self.provision_matrix_identity().await?;
            self.write_json_atomic(&self.session_path(), &session)?;
        }
        Ok("Unibox native local engine is installed and healthy.".to_string())
    }

    pub async fn start(&self) -> Result<String> {
        if self.matrix_ready().await {
            return Ok("Native Matrix runtime is already running.".to_string());
        }

        let executable = self.tuwunel_path();
        if !executable.is_file() {
            return Err(anyhow!(
                "The bundled native Matrix engine is missing ({}). Reinstall Unibox using a complete native-runtime build.",
                executable.display()
            ));
        }
        let config = self.config_path();
        if !config.is_file() {
            self.ensure_config()?;
        }

        let log = self.open_log("tuwunel.log")?;
        let stderr = log.try_clone()?;
        let mut command = Command::new(&executable);
        command.arg("-c").arg(&config);
        command.current_dir(&self.data_root);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        hide_console(&mut command);
        let child = command
            .spawn()
            .with_context(|| format!("failed to start {}", executable.display()))?;
        self.children
            .lock()
            .map_err(|_| anyhow!("runtime process lock poisoned"))?
            .homeserver = Some(child);

        for _ in 0..60 {
            if self.matrix_ready().await {
                return Ok("Native Matrix runtime started.".to_string());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        self.kill_homeserver();
        Err(anyhow!(
            "The native Matrix engine did not become ready. {}",
            self.log_tail("tuwunel.log", 5000)
        ))
    }

    pub fn stop(&self) -> Result<String> {
        self.kill_all_children();
        Ok("Native Unibox runtime stopped.".to_string())
    }

    pub fn matrix_session(&self) -> Result<MatrixSession> {
        let raw =
            fs::read_to_string(self.session_path()).context("local Matrix session is not ready")?;
        serde_json::from_str(&raw).context("invalid local Matrix session")
    }

    pub fn connector_available(&self, connector: &ConnectorDefinition) -> bool {
        is_native_bridge(connector) && self.connector_executable(connector).is_file()
    }

    pub fn connector_requirements(&self, connector: &ConnectorDefinition) -> serde_json::Value {
        if connector.id != "telegram" {
            return json!({"required": false, "fields": []});
        }
        let settings = self.load_connector_settings(connector).unwrap_or_default();
        let configured = settings.api_id.unwrap_or_default() > 0
            && settings.api_hash.as_deref().map(str::trim).map(str::len) == Some(32);
        json!({
            "required": !configured,
            "fields": [
                {
                    "id": "api_id",
                    "name": "Telegram API ID",
                    "type": "text",
                    "description": "Create an app at my.telegram.org/apps and enter its numeric API ID."
                },
                {
                    "id": "api_hash",
                    "name": "Telegram API Hash",
                    "type": "password",
                    "description": "Enter the 32-character API hash from my.telegram.org/apps."
                }
            ],
            "help": "Telegram requires each third-party client to use an API ID and API hash from my.telegram.org/apps. These values are stored only in your local Unibox data."
        })
    }

    pub fn configure_connector(
        &self,
        connector: &ConnectorDefinition,
        settings: serde_json::Value,
    ) -> Result<String> {
        self.require_native_connector(connector)?;
        if connector.id != "telegram" {
            return Ok(format!(
                "{} does not require pre-connection settings.",
                connector.name
            ));
        }

        let api_id = settings
            .get("api_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .parse::<i64>()
            .context("Telegram API ID must be a positive number")?;
        if api_id <= 0 {
            return Err(anyhow!("Telegram API ID must be a positive number"));
        }
        let api_hash = settings
            .get("api_hash")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if api_hash.len() != 32 || !api_hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(anyhow!(
                "Telegram API hash must be exactly 32 hexadecimal characters"
            ));
        }

        let state = self.connector_state(connector);
        fs::create_dir_all(&state)?;
        let local = ConnectorLocalSettings {
            api_id: Some(api_id),
            api_hash: Some(api_hash),
        };
        self.write_json_atomic(&self.connector_settings_path(connector), &local)?;

        let config = self.connector_config(connector);
        if config.is_file() {
            self.patch_connector_config(connector, &config)?;
        }
        Ok("Telegram API credentials saved locally.".to_string())
    }

    pub fn connector_status(&self, connector: &ConnectorDefinition) -> ConnectorStatus {
        let executable = self.connector_executable(connector);
        let installed = executable.is_file() && self.connector_config(connector).is_file();
        let running = installed && port_open(connector.port);
        ConnectorStatus {
            id: connector.id.clone(),
            installed,
            running,
            provisioning_url: (installed && is_native_bridge(connector))
                .then(|| format!("http://127.0.0.1:{}/_matrix/provision", connector.port)),
        }
    }

    pub async fn install_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.require_native_connector(connector)?;
        if !self.matrix_ready().await {
            self.start().await?;
        }

        let executable = self.connector_executable(connector);
        if !executable.is_file() {
            return Err(anyhow!(
                "{} is not included in this native Windows build yet.",
                connector.name
            ));
        }

        let state = self.connector_state(connector);
        fs::create_dir_all(&state)?;
        let config = self.connector_config(connector);
        let registration = state.join("registration.yaml");

        if !config.is_file() {
            self.run_connector_command(
                connector,
                &["-e", "-c"],
                Some(&config),
                "example config generation",
            )?;
        }
        self.patch_connector_config(connector, &config)?;
        if registration.exists() {
            let _ = fs::remove_file(&registration);
        }
        self.run_connector_command_with_registration(
            connector,
            &config,
            &registration,
            "appservice registration generation",
        )?;

        let appservice_target = self
            .appservice_dir()
            .join(format!("unibox-{}.yaml", connector.id));
        fs::copy(&registration, &appservice_target)
            .with_context(|| format!("failed to install {}", appservice_target.display()))?;

        // Tuwunel loads appservice_dir at startup, so restart after registrations change.
        self.kill_homeserver();
        self.start().await?;
        self.start_connector(connector).await?;
        Ok(format!("{} native connector installed.", connector.name))
    }

    pub async fn start_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.require_native_connector(connector)?;
        if port_open(connector.port) {
            return Ok(format!("{} connector is already running.", connector.name));
        }
        let executable = self.connector_executable(connector);
        let config = self.connector_config(connector);
        if !executable.is_file() || !config.is_file() {
            return Err(anyhow!(
                "{} native connector is not installed.",
                connector.name
            ));
        }

        let state = self.connector_state(connector);
        let log = self.open_connector_log(connector)?;
        let stderr = log.try_clone()?;
        let mut command = Command::new(&executable);
        command.arg("-c").arg(&config);
        command.current_dir(&state);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        hide_console(&mut command);
        let child = command
            .spawn()
            .with_context(|| format!("failed to start {}", connector.name))?;
        self.children
            .lock()
            .map_err(|_| anyhow!("runtime process lock poisoned"))?
            .connectors
            .insert(connector.id.clone(), child);

        for _ in 0..40 {
            if port_open(connector.port) {
                return Ok(format!("{} connector started.", connector.name));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        self.kill_connector(&connector.id);
        Err(anyhow!(
            "{} connector did not become ready. {}",
            connector.name,
            self.log_tail(&format!("connector-{}.log", connector.id), 4000)
        ))
    }

    pub fn stop_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.kill_connector(&connector.id);
        Ok(format!("{} connector stopped.", connector.name))
    }

    pub async fn update_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.require_native_connector(connector)?;
        Ok(format!(
            "{} connector updates are delivered by verified Unibox Windows builds.",
            connector.name
        ))
    }

    pub async fn provision_request(
        &self,
        connector: &ConnectorDefinition,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.require_native_connector(connector)?;
        let session = self.matrix_session()?;
        let base = format!("http://127.0.0.1:{}/_matrix/provision", connector.port);
        let suffix = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .context("invalid provisioning method")?;
        let mut url = Url::parse(&format!("{base}{suffix}"))
            .context("invalid local bridge provisioning URL")?;
        url.query_pairs_mut()
            .append_pair("user_id", &session.user_id);
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(session.access_token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .context("bridge provisioning request failed")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("failed to read bridge response")?;
        let value: serde_json::Value = if text.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}))
        };
        if !status.is_success() {
            return Err(anyhow!(
                "bridge provisioning returned {}: {}",
                status,
                value
            ));
        }
        Ok(value)
    }

    pub async fn client_http(&self, input: ClientHttpRequest) -> Result<ClientHttpResponse> {
        let mut url = Url::parse(&input.url).context("invalid connector client HTTP URL")?;
        let mut method = reqwest::Method::from_bytes(input.method.as_bytes())
            .context("invalid client HTTP method")?;
        let mut body = input
            .body
            .map(|value| {
                BASE64
                    .decode(value)
                    .context("invalid base64 client HTTP body")
            })
            .transpose()?;

        for _ in 0..6 {
            ensure_public_remote_url(&url).await?;
            let mut request = self.remote_http.request(method.clone(), url.clone());
            for (name, values) in &input.headers {
                if matches!(
                    name.to_ascii_lowercase().as_str(),
                    "host" | "content-length" | "connection"
                ) {
                    continue;
                }
                let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .context("invalid client HTTP header")?;
                for value in values {
                    request = request.header(header_name.clone(), value.as_str());
                }
            }
            if let Some(bytes) = &body {
                request = request.body(bytes.clone());
            }
            let response = self
                .remote_http
                .execute(request.build()?)
                .await
                .context("connector client HTTP request failed")?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .ok_or_else(|| anyhow!("redirect response is missing Location header"))?
                    .to_str()
                    .context("invalid redirect Location header")?;
                let next = url.join(location).context("invalid redirect URL")?;
                ensure_public_remote_url(&next).await?;
                if matches!(status.as_u16(), 301..=303)
                    && method != reqwest::Method::GET
                    && method != reqwest::Method::HEAD
                {
                    method = reqwest::Method::GET;
                    body = None;
                }
                url = next;
                continue;
            }
            let status_code = status.as_u16();
            let final_url = response.url().to_string();
            let mut headers: HashMap<String, Vec<String>> = HashMap::new();
            for (name, value) in response.headers() {
                if let Ok(value) = value.to_str() {
                    headers
                        .entry(name.as_str().to_string())
                        .or_default()
                        .push(value.to_string());
                }
            }
            let bytes = response
                .bytes()
                .await
                .context("failed to read client HTTP response")?;
            return Ok(ClientHttpResponse {
                status_code,
                final_url,
                headers,
                body: BASE64.encode(bytes),
            });
        }
        Err(anyhow!("connector client HTTP exceeded redirect limit"))
    }

    fn prepare_layout(&self) -> Result<()> {
        for path in [
            self.matrix_dir(),
            self.appservice_dir(),
            self.data_root.join("connectors"),
            self.data_root.join("logs"),
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }

    fn ensure_config(&self) -> Result<()> {
        self.prepare_layout()?;
        let secrets = self.load_or_create_secrets()?;
        let config = self.config_path();
        let db = slash_path(&self.matrix_dir());
        let appservices = slash_path(&self.appservice_dir());
        let content = format!(
            "[global]\nserver_name = \"unibox.local\"\ndatabase_path = \"{}\"\naddress = [\"127.0.0.1\"]\nport = 8008\nallow_federation = false\nallow_registration = false\ncreate_admin_room = false\nnew_user_displayname_suffix = \"\"\nappservice_dir = \"{}\"\nregistration_shared_secret = \"{}\"\nlog = \"warn,tuwunel=info\"\n",
            toml_escape(&db),
            toml_escape(&appservices),
            toml_escape(&secrets.registration_shared_secret)
        );
        fs::write(config, content)?;
        Ok(())
    }

    fn load_or_create_secrets(&self) -> Result<NativeSecrets> {
        let path = self.secrets_path();
        if path.is_file() {
            return serde_json::from_str(&fs::read_to_string(path)?)
                .context("invalid native runtime secrets");
        }
        let secrets = NativeSecrets {
            registration_shared_secret: random_secret(64),
            matrix_password: random_secret(48),
        };
        self.write_json_atomic(&path, &secrets)?;
        Ok(secrets)
    }

    async fn provision_matrix_identity(&self) -> Result<MatrixSession> {
        let secrets = self.load_or_create_secrets()?;
        let register_url = format!("{MATRIX_URL}/_synapse/admin/v1/register");
        let nonce_response = self
            .http
            .get(&register_url)
            .send()
            .await
            .context("failed to request local registration nonce")?;
        if !nonce_response.status().is_success() {
            return Err(anyhow!(
                "local Matrix nonce request failed with {}",
                nonce_response.status()
            ));
        }
        let nonce_json: serde_json::Value = nonce_response.json().await?;
        let nonce = nonce_json
            .get("nonce")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("local Matrix registration nonce is missing"))?;
        let payload = format!(
            "{nonce}\0{MATRIX_USER}\0{}\0notadmin",
            secrets.matrix_password
        );
        let mut mac = HmacSha1::new_from_slice(secrets.registration_shared_secret.as_bytes())?;
        mac.update(payload.as_bytes());
        let mac = hex::encode(mac.finalize().into_bytes());

        let register = self
            .http
            .post(&register_url)
            .json(&json!({
                "nonce": nonce,
                "username": MATRIX_USER,
                "password": secrets.matrix_password,
                "admin": false,
                "mac": mac,
                "displayname": "Unibox"
            }))
            .send()
            .await
            .context("failed to create local Matrix identity")?;

        if register.status().is_success() {
            let value: serde_json::Value = register.json().await?;
            return session_from_registration(value);
        }

        // The user may already exist after an interrupted install; log in using
        // the password persisted in secrets.json instead of destroying data.
        let login = self
            .http
            .post(format!("{MATRIX_URL}/_matrix/client/v3/login"))
            .json(&json!({
                "type": "m.login.password",
                "identifier": {"type": "m.id.user", "user": MATRIX_USER},
                "password": secrets.matrix_password,
                "initial_device_display_name": "Unibox Desktop"
            }))
            .send()
            .await
            .context("failed to log in to local Matrix identity")?;
        let status = login.status();
        let value: serde_json::Value = login.json().await.unwrap_or_else(|_| json!({}));
        if !status.is_success() {
            return Err(anyhow!(
                "local Matrix identity provisioning failed with {}: {}",
                status,
                value
            ));
        }
        session_from_login(value)
    }

    async fn matrix_ready(&self) -> bool {
        self.http
            .get(format!("{MATRIX_URL}/_matrix/client/versions"))
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    fn require_native_connector(&self, connector: &ConnectorDefinition) -> Result<()> {
        if !is_native_bridge(connector) {
            return Err(anyhow!(
                "{} is not using the native BridgeV2 adapter in this Windows build.",
                connector.name
            ));
        }
        if !self.connector_executable(connector).is_file() {
            return Err(anyhow!(
                "{} native connector binary is not bundled in this build.",
                connector.name
            ));
        }
        Ok(())
    }

    fn patch_connector_config(&self, connector: &ConnectorDefinition, path: &Path) -> Result<()> {
        let raw = fs::read_to_string(path)?;
        let mut cfg: YamlValue =
            serde_yaml::from_str(&raw).context("invalid connector config YAML")?;
        let root = cfg
            .as_mapping_mut()
            .ok_or_else(|| anyhow!("connector config root is not a mapping"))?;

        let homeserver = mapping_child(root, "homeserver");
        set_yaml(
            homeserver,
            "address",
            YamlValue::String(MATRIX_URL.to_string()),
        );
        set_yaml(
            homeserver,
            "domain",
            YamlValue::String("unibox.local".to_string()),
        );
        set_yaml(
            homeserver,
            "software",
            YamlValue::String("standard".to_string()),
        );

        let appservice = mapping_child(root, "appservice");
        set_yaml(
            appservice,
            "address",
            YamlValue::String(format!("http://127.0.0.1:{}", connector.port)),
        );
        set_yaml(
            appservice,
            "hostname",
            YamlValue::String("127.0.0.1".to_string()),
        );
        set_yaml(appservice, "port", YamlValue::Number(connector.port.into()));
        set_yaml(
            appservice,
            "id",
            YamlValue::String(format!("unibox-{}", connector.id)),
        );

        let database = mapping_child(root, "database");
        set_yaml(
            database,
            "type",
            YamlValue::String("sqlite3-fk-wal".to_string()),
        );
        let db = slash_path(&self.connector_state(connector).join("bridge.db"));
        set_yaml(
            database,
            "uri",
            YamlValue::String(format!("file:{db}?_txlock=immediate")),
        );
        set_yaml(database, "max_open_conns", YamlValue::Number(1.into()));
        set_yaml(database, "max_idle_conns", YamlValue::Number(1.into()));

        let bridge = mapping_child(root, "bridge");
        let mut permissions = Mapping::new();
        permissions.insert(
            YamlValue::String("@unibox:unibox.local".to_string()),
            YamlValue::String("admin".to_string()),
        );
        set_yaml(bridge, "permissions", YamlValue::Mapping(permissions));

        if connector.id == "telegram" {
            let settings = self.load_connector_settings(connector)?;
            let api_id = settings
                .api_id
                .filter(|value| *value > 0)
                .ok_or_else(|| anyhow!("Telegram API ID is not configured"))?;
            let api_hash = settings
                .api_hash
                .filter(|value| value.len() == 32 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
                .ok_or_else(|| anyhow!("Telegram API hash is not configured"))?;
            set_yaml(root, "api_id", YamlValue::Number(api_id.into()));
            set_yaml(root, "api_hash", YamlValue::String(api_hash));
        }

        if let Some(matrix) = mapping_child_optional(root, "matrix") {
            set_yaml(matrix, "federate_rooms", YamlValue::Bool(false));
        }
        if let Some(encryption) = mapping_child_optional(root, "encryption") {
            set_yaml(encryption, "allow", YamlValue::Bool(false));
            set_yaml(encryption, "default", YamlValue::Bool(false));
        }
        if let Some(provisioning) = mapping_child_optional(root, "provisioning") {
            set_yaml(provisioning, "allow_matrix_auth", YamlValue::Bool(true));
        }

        fs::write(path, serde_yaml::to_string(&cfg)?)?;
        Ok(())
    }

    fn run_connector_command(
        &self,
        connector: &ConnectorDefinition,
        prefix: &[&str],
        path_arg: Option<&Path>,
        label: &str,
    ) -> Result<()> {
        let executable = self.connector_executable(connector);
        let mut command = Command::new(&executable);
        command.args(prefix);
        if let Some(path) = path_arg {
            command.arg(path);
        }
        command.current_dir(self.connector_state(connector));
        command.stdin(Stdio::null());
        hide_console(&mut command);
        let output = command
            .output()
            .with_context(|| format!("failed to run {label} for {}", connector.name))?;
        if !output.status.success() {
            return Err(anyhow!(
                "{} {} failed: {}{}",
                connector.name,
                label,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    fn run_connector_command_with_registration(
        &self,
        connector: &ConnectorDefinition,
        config: &Path,
        registration: &Path,
        label: &str,
    ) -> Result<()> {
        let executable = self.connector_executable(connector);
        let mut command = Command::new(&executable);
        command
            .arg("-g")
            .arg("-c")
            .arg(config)
            .arg("-r")
            .arg(registration);
        command.current_dir(self.connector_state(connector));
        command.stdin(Stdio::null());
        hide_console(&mut command);
        let output = command
            .output()
            .with_context(|| format!("failed to run {label} for {}", connector.name))?;
        if !output.status.success() {
            return Err(anyhow!(
                "{} {} failed: {}{}",
                connector.name,
                label,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    fn write_json_atomic<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        fs::rename(tmp, path)?;
        Ok(())
    }

    fn open_log(&self, name: &str) -> Result<File> {
        let dir = self.data_root.join("logs");
        fs::create_dir_all(&dir)?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(name))
            .map_err(Into::into)
    }

    fn open_connector_log(&self, connector: &ConnectorDefinition) -> Result<File> {
        self.open_log(&format!("connector-{}.log", connector.id))
    }

    fn log_tail(&self, name: &str, max: usize) -> String {
        let path = self.data_root.join("logs").join(name);
        let mut bytes = Vec::new();
        if File::open(path)
            .and_then(|mut f| f.read_to_end(&mut bytes))
            .is_err()
        {
            return "See the Unibox native runtime logs for details.".to_string();
        }
        let text = String::from_utf8_lossy(&bytes);
        let start = text.len().saturating_sub(max);
        format!("Last log output: {}", &text[start..])
    }

    fn kill_homeserver(&self) {
        if let Ok(mut children) = self.children.lock() {
            if let Some(mut child) = children.homeserver.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn kill_connector(&self, id: &str) {
        if let Ok(mut children) = self.children.lock() {
            if let Some(mut child) = children.connectors.remove(id) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn kill_all_children(&self) {
        if let Ok(mut children) = self.children.lock() {
            for (_, mut child) in children.connectors.drain() {
                let _ = child.kill();
                let _ = child.wait();
            }
            if let Some(mut child) = children.homeserver.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn tuwunel_path(&self) -> PathBuf {
        self.resource_root.join("tuwunel.exe")
    }
    fn matrix_dir(&self) -> PathBuf {
        self.data_root.join("matrix")
    }
    fn appservice_dir(&self) -> PathBuf {
        self.data_root.join("appservices")
    }
    fn config_path(&self) -> PathBuf {
        self.data_root.join("tuwunel.toml")
    }
    fn secrets_path(&self) -> PathBuf {
        self.data_root.join("secrets.json")
    }
    fn session_path(&self) -> PathBuf {
        self.data_root.join("matrix-session.json")
    }
    fn load_connector_settings(
        &self,
        connector: &ConnectorDefinition,
    ) -> Result<ConnectorLocalSettings> {
        let path = self.connector_settings_path(connector);
        if !path.is_file() {
            return Ok(ConnectorLocalSettings::default());
        }
        serde_json::from_str(&fs::read_to_string(&path)?)
            .with_context(|| format!("invalid local settings for {}", connector.name))
    }

    fn connector_settings_path(&self, connector: &ConnectorDefinition) -> PathBuf {
        self.connector_state(connector).join("unibox-settings.json")
    }

    fn connector_state(&self, connector: &ConnectorDefinition) -> PathBuf {
        self.data_root.join("connectors").join(&connector.id)
    }
    fn connector_config(&self, connector: &ConnectorDefinition) -> PathBuf {
        self.connector_state(connector).join("config.yaml")
    }
    fn connector_executable(&self, connector: &ConnectorDefinition) -> PathBuf {
        self.resource_root
            .join("connectors")
            .join(format!("{}.exe", connector.binary))
    }
}

impl Drop for NativeRuntimeManager {
    fn drop(&mut self) {
        self.kill_all_children();
    }
}

fn session_from_registration(value: serde_json::Value) -> Result<MatrixSession> {
    let user_id = value
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("registration response missing user_id"))?;
    let access_token = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("registration response missing access_token"))?;
    Ok(MatrixSession {
        homeserver: MATRIX_URL.to_string(),
        user_id: user_id.to_string(),
        access_token: access_token.to_string(),
        device_id: value
            .get("device_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

fn session_from_login(value: serde_json::Value) -> Result<MatrixSession> {
    let user_id = value
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("login response missing user_id"))?;
    let access_token = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("login response missing access_token"))?;
    Ok(MatrixSession {
        homeserver: MATRIX_URL.to_string(),
        user_id: user_id.to_string(),
        access_token: access_token.to_string(),
        device_id: value
            .get("device_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

fn random_secret(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn mapping_child<'a>(root: &'a mut Mapping, key: &str) -> &'a mut Mapping {
    let k = YamlValue::String(key.to_string());
    if !matches!(root.get(&k), Some(YamlValue::Mapping(_))) {
        root.insert(k.clone(), YamlValue::Mapping(Mapping::new()));
    }
    root.get_mut(&k)
        .and_then(YamlValue::as_mapping_mut)
        .expect("mapping inserted")
}

fn mapping_child_optional<'a>(root: &'a mut Mapping, key: &str) -> Option<&'a mut Mapping> {
    root.get_mut(YamlValue::String(key.to_string()))
        .and_then(YamlValue::as_mapping_mut)
}

fn set_yaml(map: &mut Mapping, key: &str, value: YamlValue) {
    map.insert(YamlValue::String(key.to_string()), value);
}

fn port_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(200),
    )
    .is_ok()
}

fn is_native_bridge(connector: &ConnectorDefinition) -> bool {
    matches!(connector.adapter.as_str(), "bridgev2" | "source-go")
}

#[cfg(target_os = "windows")]
fn stop_legacy_native_slot_processes(resource_root: &Path) {
    let Some(resources_root) = resource_root.parent() else {
        return;
    };
    let current = resource_root.to_string_lossy().replace('\'', "''");
    let parent = resources_root.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$current='{current}'; $parent='{parent}'; Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {{ $_.ExecutablePath -and $_.ExecutablePath.StartsWith($parent,[System.StringComparison]::OrdinalIgnoreCase) -and -not $_.ExecutablePath.StartsWith($current,[System.StringComparison]::OrdinalIgnoreCase) -and ((Split-Path $_.ExecutablePath -Leaf) -eq 'tuwunel.exe' -or (Split-Path $_.ExecutablePath -Leaf) -like 'mautrix-*.exe') }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}"
    );

    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console(&mut command);
    let _ = command.status();
    std::thread::sleep(Duration::from_millis(700));
}

#[cfg(not(target_os = "windows"))]
fn stop_legacy_native_slot_processes(_resource_root: &Path) {}

fn hide_console(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = command;
    }
}

async fn ensure_public_remote_url(url: &Url) -> Result<()> {
    if url.scheme() != "https" {
        return Err(anyhow!(
            "connector client HTTP only permits public HTTPS URLs"
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("connector client HTTP URL is missing a host"))?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return Err(anyhow!("connector client HTTP blocks local hosts"));
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(anyhow!(
                "connector client HTTP blocks private or local addresses"
            ));
        }
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let resolved: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("failed to resolve connector HTTP host {host}"))?
        .collect();
    if resolved.is_empty() || resolved.iter().any(|address| is_private_ip(address.ip())) {
        return Err(anyhow!(
            "connector client HTTP host resolves to a private/local or empty address set"
        ));
    }
    Ok(())
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}
