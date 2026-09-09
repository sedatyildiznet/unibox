use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::process::{Command, Output};
use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(target_os = "windows")]
fn windows_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW; elevation remains visible.
    command
}

pub const DISTRO_NAME: &str = "UniboxRuntime";
pub const MATRIX_URL: &str = "http://127.0.0.1:8008";
pub const SYNAPSE_SERVICE: &str = "unibox-synapse";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixSession {
    pub homeserver: String,
    pub user_id: String,
    pub access_token: String,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub bootstrap: Option<BootstrapResult>,
    pub platform: String,
    pub wsl_available: bool,
    pub distro_installed: bool,
    pub distro_running: bool,
    pub synapse_ready: bool,
    pub matrix_session_ready: bool,
    pub data_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BootstrapState {
    InstallingWsl,
    RebootRequired,
    RuntimeInstalling,
    RuntimeReady,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResult {
    pub state: BootstrapState,
    pub message: String,
}

pub fn parse_bootstrap_output(text: &str) -> Result<BootstrapResult> {
    let line = text
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("UNIBOX_BOOTSTRAP:"))
        .ok_or_else(|| anyhow!("Local engine setup ended unexpectedly. Please retry."))?;
    serde_json::from_str(line)
        .context("Local engine returned an invalid setup result. Please retry.")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorDefinition {
    pub id: String,
    pub name: String,
    pub category: String,
    pub repository: String,
    pub binary: String,
    pub asset_linux_amd64: String,
    pub port: u16,
    pub maturity: String,
    pub adapter: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub multi_account: bool,
    #[serde(default)]
    pub risk_notice: Option<String>,
    #[serde(default)]
    pub bot_localpart: Option<String>,
    #[serde(default)]
    pub legacy_login: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorStatus {
    pub id: String,
    pub installed: bool,
    pub running: bool,
    pub provisioning_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHttpRequest {
    pub request_id: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHttpResponse {
    pub status_code: u16,
    pub final_url: String,
    pub headers: HashMap<String, Vec<String>>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeManager {
    data_root: PathBuf,
    http: Client,
    provision_http: Client,
}

impl RuntimeManager {
    pub fn new(data_root: impl Into<PathBuf>) -> Result<Self> {
        let data_root = data_root.into();
        std::fs::create_dir_all(&data_root)
            .with_context(|| format!("failed to create {}", data_root.display()))?;
        Ok(Self {
            data_root,
            http: Client::builder().timeout(Duration::from_secs(3)).build()?,
            provision_http: Client::builder().timeout(Duration::from_secs(70)).build()?,
        })
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub async fn status(&self) -> RuntimeStatus {
        let wsl_available = self.wsl_available();
        let distro_installed = wsl_available && self.distro_installed();
        let distro_running = distro_installed && self.distro_running();
        let synapse_ready = self.synapse_ready().await;
        let matrix_session_ready = distro_installed && self.matrix_session().is_ok();

        RuntimeStatus {
            bootstrap: std::fs::read(self.data_root.join("bootstrap-state.json"))
                .ok()
                .and_then(|raw| serde_json::from_slice(&raw).ok()),
            platform: std::env::consts::OS.to_string(),
            wsl_available,
            distro_installed,
            distro_running,
            synapse_ready,
            matrix_session_ready,
            data_root: self.data_root.display().to_string(),
        }
    }

    pub fn bootstrap(&self, script: &Path) -> Result<BootstrapResult> {
        #[cfg(target_os = "windows")]
        {
            let output = windows_command("powershell.exe")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(script)
                .arg("-DataRoot")
                .arg(&self.data_root)
                .output()
                .context("failed to start Unibox runtime bootstrap")?;
            let result = parse_bootstrap_output(&decode_output(&output.stdout))?;
            if !output.status.success() && !matches!(result.state, BootstrapState::Error) {
                return Err(anyhow!(
                    "Local engine setup ended unexpectedly. Please retry."
                ));
            }
            Ok(result)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = script;
            Err(anyhow!(
                "managed runtime bootstrap is currently implemented for Windows"
            ))
        }
    }

    pub fn restart_windows(&self) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let state: BootstrapResult = serde_json::from_slice(&std::fs::read(
                self.data_root.join("bootstrap-state.json"),
            )?)?;
            if !matches!(state.state, BootstrapState::RebootRequired) {
                return Err(anyhow!("A Windows restart is not required by setup."));
            }
            let output = windows_command("shutdown.exe")
                .args(["/r", "/t", "0"])
                .output()
                .context("Windows could not restart. Please restart from the Start menu.")?;
            if !output.status.success() {
                return Err(anyhow!(
                    "Windows could not restart. Please restart from the Start menu."
                ));
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        Err(anyhow!("Restart is only available on Windows."))
    }

    pub fn start(&self) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let shell = format!(
                "systemctl start postgresql {0} && systemctl is-active --quiet postgresql {0} && for i in $(seq 1 60); do curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null && exit 0; sleep 1; done; exit 1",
                SYNAPSE_SERVICE
            );
            let output = windows_command("wsl.exe")
                .args(["-d", DISTRO_NAME, "--", "bash", "-lc", &shell])
                .output()
                .context("failed to start UniboxRuntime")?;
            output_text(output, "failed to start local services")
        }
        #[cfg(not(target_os = "windows"))]
        Err(anyhow!(
            "managed runtime start is currently implemented for Windows"
        ))
    }

    pub fn stop(&self) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let output = windows_command("wsl.exe")
                .args(["--terminate", DISTRO_NAME])
                .output()
                .context("failed to terminate UniboxRuntime")?;
            output_text(output, "failed to stop local runtime")
        }
        #[cfg(not(target_os = "windows"))]
        Err(anyhow!(
            "managed runtime stop is currently implemented for Windows"
        ))
    }

    pub fn matrix_session(&self) -> Result<MatrixSession> {
        let raw = self.wsl_capture("cat /var/lib/unibox/matrix.json")?;
        let session: MatrixSession =
            serde_json::from_str(raw.trim()).context("invalid local Matrix session")?;
        if session.homeserver != MATRIX_URL
            || session.access_token.is_empty()
            || session.access_token.contains(['\r', '\n'])
            || !session.user_id.ends_with(":unibox.local")
        {
            return Err(anyhow!(
                "The stored local session is invalid. Restore a backup or repair the local engine."
            ));
        }
        Ok(session)
    }

    pub fn connector_status(&self, connector: &ConnectorDefinition) -> ConnectorStatus {
        let installed = if connector.adapter == "python-legacy" {
            self.wsl_ok(&format!(
                "test -f /opt/unibox/connectors/{}/.installed",
                connector.id
            ))
        } else {
            self.wsl_ok(&format!(
                "test -x /opt/unibox/connectors/{0}/{1}",
                connector.id, connector.binary
            ))
        };
        let running = installed
            && self.wsl_ok(&format!(
                "systemctl is-active --quiet unibox-{}.service",
                connector.id
            ));
        ConnectorStatus {
            id: connector.id.clone(),
            installed,
            running,
            provisioning_url: (installed
                && matches!(connector.adapter.as_str(), "bridgev2" | "source-go"))
            .then(|| format!("http://127.0.0.1:{}/_matrix/provision", connector.port)),
        }
    }

    pub fn install_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        let port = connector.port.to_string();
        let args = [
            "install",
            connector.id.as_str(),
            connector.adapter.as_str(),
            connector.repository.as_str(),
            connector.binary.as_str(),
            connector.asset_linux_amd64.as_str(),
            port.as_str(),
        ];
        self.connector_command(&args)
    }

    pub fn start_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.connector_command(&["start", connector.id.as_str()])
    }

    pub fn stop_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.connector_command(&["stop", connector.id.as_str()])
    }

    pub async fn provision_request(
        &self,
        connector: &ConnectorDefinition,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        if connector.adapter != "bridgev2" && connector.adapter != "source-go" {
            return Err(anyhow!(
                "{} does not use the BridgeV2 provisioning API",
                connector.name
            ));
        }
        let session = self.matrix_session()?;
        let base = format!("http://127.0.0.1:{}/_matrix/provision", connector.port);
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .context("invalid provisioning method")?;
        let mut request = self
            .provision_http
            .request(method, format!("{base}{path}"))
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
            serde_json::json!({})
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({"raw": text}))
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

    pub fn update_connector(&self, connector: &ConnectorDefinition) -> Result<String> {
        self.connector_command(&[
            "update",
            connector.id.as_str(),
            connector.adapter.as_str(),
            connector.repository.as_str(),
            connector.binary.as_str(),
            connector.asset_linux_amd64.as_str(),
        ])
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

        let original_origin = url.origin();
        for _ in 0..6 {
            let addresses = ensure_public_remote_url(&url).await?;
            // Pin the validated DNS result for the actual connection. A second DNS lookup
            // would let an attacker switch a public address to loopback after validation.
            let host = url
                .host_str()
                .ok_or_else(|| anyhow!("Missing remote host"))?;
            let client = Client::builder()
                .timeout(Duration::from_secs(70))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .resolve_to_addrs(host, &addresses)
                .build()?;
            let mut request = client.request(method.clone(), url.clone());
            for (name, values) in &input.headers {
                if url.origin() != original_origin {
                    // Connector-supplied headers may contain secrets under arbitrary names.
                    continue;
                }
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

            let response = client
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
                if next.origin() != url.origin() && body.is_some() {
                    return Err(anyhow!(
                        "Cross-origin redirects with authentication bodies are not allowed"
                    ));
                }

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

    fn connector_command(&self, args: &[&str]) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let mut command = windows_command("wsl.exe");
            command.args(["-d", DISTRO_NAME, "--", "/opt/unibox/bin/unibox-connector"]);
            command.args(args);
            let output = command
                .output()
                .context("failed to execute connector manager")?;
            output_text(output, "connector operation failed")
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = args;
            Err(anyhow!(
                "managed connector operations are currently implemented for Windows"
            ))
        }
    }

    fn wsl_available(&self) -> bool {
        #[cfg(target_os = "windows")]
        return windows_command("wsl.exe")
            .arg("--status")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        #[cfg(not(target_os = "windows"))]
        false
    }

    fn distro_installed(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            let out = windows_command("wsl.exe").args(["-l", "-q"]).output();
            out.ok()
                .map(|o| {
                    decode_output(&o.stdout)
                        .lines()
                        .any(|x| x.trim() == DISTRO_NAME)
                })
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "windows"))]
        false
    }

    fn distro_running(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            let out = windows_command("wsl.exe")
                .args(["--list", "--running", "--quiet"])
                .output();
            out.ok()
                .map(|o| {
                    decode_output(&o.stdout)
                        .lines()
                        .any(|line| line.trim() == DISTRO_NAME)
                })
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "windows"))]
        false
    }

    async fn synapse_ready(&self) -> bool {
        self.http
            .get(format!("{MATRIX_URL}/_matrix/client/versions"))
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    fn wsl_capture(&self, shell: &str) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let output = windows_command("wsl.exe")
                .args(["-d", DISTRO_NAME, "--", "bash", "-lc", shell])
                .output()
                .context("failed to execute command in UniboxRuntime")?;
            output_text(output, "UniboxRuntime command failed")
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = shell;
            Err(anyhow!(
                "managed runtime is currently implemented for Windows"
            ))
        }
    }

    fn wsl_ok(&self, shell: &str) -> bool {
        self.wsl_capture(shell).is_ok()
    }
}

pub fn parse_registry(raw: &str) -> Result<Vec<ConnectorDefinition>> {
    serde_json::from_str(raw).context("invalid connector registry")
}

async fn ensure_public_remote_url(url: &Url) -> Result<Vec<std::net::SocketAddr>> {
    if !is_allowed_remote_url(url) {
        return Err(anyhow!(
            "connector client HTTP only permits public HTTPS URLs"
        ));
    }

    let Some(host) = url.host_str() else {
        return Err(anyhow!("connector client HTTP URL is missing a host"));
    };
    let port = url.port_or_known_default().unwrap_or(443);
    if let Some(ip) = parse_host_ip(host) {
        return Ok(vec![std::net::SocketAddr::new(ip, port)]);
    }

    let resolved: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("failed to resolve connector HTTP host {host}"))?
        .collect();
    if resolved.is_empty() {
        return Err(anyhow!("connector HTTP host did not resolve"));
    }
    if resolved.iter().any(|address| is_private_ip(address.ip())) {
        return Err(anyhow!(
            "connector client HTTP host resolves to a private or local address"
        ));
    }
    Ok(resolved)
}

fn is_allowed_remote_url(url: &Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return false;
    }
    parse_host_ip(host)
        .map(|ip| !is_private_ip(ip))
        .unwrap_or(true)
}

fn parse_host_ip(host: &str) -> Option<IpAddr> {
    let normalized = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    normalized.parse::<IpAddr>().ok()
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || octets[0] == 0
                || octets[0] >= 240
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_private_ip(IpAddr::V4(mapped));
            }
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (ip.segments()[0] & 0xe000) != 0x2000
                || ip.segments()[0] == 0x2002
                || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0)
                || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0xdb8)
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(target_os = "windows")]
fn output_text(output: Output, message: &str) -> Result<String> {
    if output.status.success() {
        Ok(decode_output(&output.stdout))
    } else {
        let stderr = decode_output(&output.stderr);
        let stdout = decode_output(&output.stdout);
        Err(anyhow!("{}: {}{}", message, stdout, stderr))
    }
}

#[cfg(any(test, target_os = "windows"))]
fn decode_output(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if bytes.starts_with(&[0xff, 0xfe]) || (bytes.len() >= 2 && bytes[1] == 0) {
        let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
        let mut words = Vec::with_capacity(bytes.len() / 2);
        let mut index = 0;
        while index + 1 < bytes.len() {
            words.push(u16::from_le_bytes([bytes[index], bytes[index + 1]]));
            index += 2;
        }
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_http_rejects_mapped_loopback_and_special_networks() {
        for raw in [
            "https://[::ffff:127.0.0.1]/",
            "https://[::ffff:10.0.0.1]/",
            "https://100.64.0.1/",
            "https://224.0.0.1/",
            "https://0.1.2.3/",
            "https://[64:ff9b::7f00:1]/",
            "https://user:secret@example.com/",
        ] {
            assert!(!is_allowed_remote_url(&Url::parse(raw).unwrap()), "{raw}");
        }
        assert!(is_allowed_remote_url(
            &Url::parse("https://[2606:4700:4700::1111]/").unwrap()
        ));
    }

    #[test]
    fn reboot_result_is_not_an_error() {
        let result = parse_bootstrap_output("noise\nUNIBOX_BOOTSTRAP:{\"state\":\"REBOOT_REQUIRED\",\"message\":\"Restart Windows\"}\n").unwrap();
        assert!(matches!(result.state, BootstrapState::RebootRequired));
    }

    #[test]
    fn bootstrap_requires_a_known_final_result() {
        assert!(parse_bootstrap_output("PowerShell stack trace").is_err());
        assert!(parse_bootstrap_output(
            "UNIBOX_BOOTSTRAP:{\"state\":\"UNKNOWN\",\"message\":\"x\"}"
        )
        .is_err());
        let result = parse_bootstrap_output("UNIBOX_BOOTSTRAP:{\"state\":\"RUNTIME_INSTALLING\",\"message\":\"Preparing\"}\nUNIBOX_BOOTSTRAP:{\"state\":\"ERROR\",\"message\":\"Retry\"}").unwrap();
        assert!(matches!(result.state, BootstrapState::Error));
    }

    #[test]
    fn remote_url_policy_blocks_local_targets() {
        for value in [
            "http://example.com",
            "https://localhost/test",
            "https://service.local/test",
            "https://127.0.0.1/test",
            "https://10.0.0.1/test",
            "https://169.254.1.1/test",
            "https://[::1]/test",
            "https://[::]/test",
            "https://[fc00::1]/test",
            "https://[fe80::1]/test",
        ] {
            let url = Url::parse(value).unwrap();
            assert!(!is_allowed_remote_url(&url), "should block {value}");
        }
    }

    #[test]
    fn remote_url_policy_allows_public_https_targets() {
        for value in ["https://example.com", "https://8.8.8.8/dns-query"] {
            let url = Url::parse(value).unwrap();
            assert!(is_allowed_remote_url(&url), "should allow {value}");
        }
    }

    #[test]
    fn registry_parser_rejects_invalid_json() {
        assert!(parse_registry("not json").is_err());
    }

    #[test]
    fn utf16_output_decoder_handles_wsl_listing() {
        let text = "UniboxRuntime\r\n";
        let bytes: Vec<u8> = text
            .encode_utf16()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        assert_eq!(decode_output(&bytes), text);
    }

    #[test]
    fn synapse_service_name_matches_managed_runtime() {
        assert_eq!(SYNAPSE_SERVICE, "unibox-synapse");
    }
}
