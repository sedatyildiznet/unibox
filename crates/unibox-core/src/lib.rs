use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};

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
    pub platform: String,
    pub wsl_available: bool,
    pub distro_installed: bool,
    pub distro_running: bool,
    pub synapse_ready: bool,
    pub matrix_session_ready: bool,
    pub data_root: String,
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
    remote_http: Client,
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
            remote_http: Client::builder()
                .timeout(Duration::from_secs(70))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
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
            platform: std::env::consts::OS.to_string(),
            wsl_available,
            distro_installed,
            distro_running,
            synapse_ready,
            matrix_session_ready,
            data_root: self.data_root.display().to_string(),
        }
    }

    pub fn bootstrap(&self, script: &Path) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("powershell.exe")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(script)
                .arg("-DataRoot")
                .arg(&self.data_root)
                .output()
                .context("failed to start Unibox runtime bootstrap")?;
            return output_text(output, "runtime bootstrap failed");
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = script;
            Err(anyhow!(
                "managed runtime bootstrap is currently implemented for Windows"
            ))
        }
    }

    pub fn start(&self) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let shell = format!(
                "systemctl start postgresql {0} && systemctl is-active postgresql {0}",
                SYNAPSE_SERVICE
            );
            let output = Command::new("wsl.exe")
                .args(["-d", DISTRO_NAME, "--", "bash", "-lc", &shell])
                .output()
                .context("failed to start UniboxRuntime")?;
            return output_text(output, "failed to start local services");
        }
        #[cfg(not(target_os = "windows"))]
        Err(anyhow!(
            "managed runtime start is currently implemented for Windows"
        ))
    }

    pub fn stop(&self) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("wsl.exe")
                .args(["--terminate", DISTRO_NAME])
                .output()
                .context("failed to terminate UniboxRuntime")?;
            return output_text(output, "failed to stop local runtime");
        }
        #[cfg(not(target_os = "windows"))]
        Err(anyhow!(
            "managed runtime stop is currently implemented for Windows"
        ))
    }

    pub fn matrix_session(&self) -> Result<MatrixSession> {
        let raw = self.wsl_capture("cat /var/lib/unibox/matrix.json")?;
        serde_json::from_str(raw.trim()).context("invalid local Matrix session")
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
            provisioning_url: (installed && connector.adapter == "bridgev2")
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
        if connector.adapter != "bridgev2" {
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

                if matches!(status.as_u16(), 301 | 302 | 303)
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
            let mut command = Command::new("wsl.exe");
            command.args(["-d", DISTRO_NAME, "--", "/opt/unibox/bin/unibox-connector"]);
            command.args(args);
            let output = command
                .output()
                .context("failed to execute connector manager")?;
            return output_text(output, "connector operation failed");
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
        return Command::new("wsl.exe")
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
            let out = Command::new("wsl.exe").args(["-l", "-q"]).output();
            return out
                .ok()
                .map(|o| {
                    decode_output(&o.stdout)
                        .lines()
                        .any(|x| x.trim() == DISTRO_NAME)
                })
                .unwrap_or(false);
        }
        #[cfg(not(target_os = "windows"))]
        false
    }

    fn distro_running(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            let out = Command::new("wsl.exe").args(["-l", "-v"]).output();
            return out
                .ok()
                .map(|o| {
                    decode_output(&o.stdout).lines().any(|line| {
                        line.contains(DISTRO_NAME) && line.to_ascii_lowercase().contains("running")
                    })
                })
                .unwrap_or(false);
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
            let output = Command::new("wsl.exe")
                .args(["-d", DISTRO_NAME, "--", "bash", "-lc", shell])
                .output()
                .context("failed to execute command in UniboxRuntime")?;
            return output_text(output, "UniboxRuntime command failed");
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

async fn ensure_public_remote_url(url: &Url) -> Result<()> {
    if !is_allowed_remote_url(url) {
        return Err(anyhow!(
            "connector client HTTP only permits public HTTPS URLs"
        ));
    }

    let Some(host) = url.host_str() else {
        return Err(anyhow!("connector client HTTP URL is missing a host"));
    };
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    let port = url.port_or_known_default().unwrap_or(443);
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
    Ok(())
}

fn is_allowed_remote_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return false;
    }
    host.parse::<IpAddr>()
        .map(|ip| !is_private_ip(ip))
        .unwrap_or(true)
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

fn output_text(output: Output, message: &str) -> Result<String> {
    if output.status.success() {
        Ok(decode_output(&output.stdout))
    } else {
        let stderr = decode_output(&output.stderr);
        let stdout = decode_output(&output.stdout);
        Err(anyhow!("{}: {}{}", message, stdout, stderr))
    }
}

fn decode_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[1] == 0 {
        let words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "https://[fc00::1]/test",
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
