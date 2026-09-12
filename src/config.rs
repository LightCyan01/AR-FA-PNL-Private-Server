use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
    pub assets: AssetConfig,
    pub versions: VersionConfig,
    pub paths: PathConfig,
    pub storage: StorageConfig,
    pub session: SessionConfig,
    pub transport: TransportConfig,
    pub logging: LoggingConfig,
    pub account: AccountConfig,
    #[serde(default)]
    pub payment: PaymentConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetConfig {
    pub bind: String,
    pub url_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionConfig {
    pub client: String,
    pub asset: String,
    pub master_data: String,
    #[serde(default)]
    pub terms_of_service: String,
    #[serde(default)]
    pub privacy_policy: String,
    #[serde(default = "default_true")]
    pub enforce: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathConfig {
    pub protoset: PathBuf,
    pub master_data: PathBuf,
    pub master_data_download: PathBuf,
    #[serde(default)]
    pub master_data_decoded: Option<PathBuf>,
    pub manifest: PathBuf,
    pub catalog: PathBuf,
    pub asset_root: PathBuf,
    #[serde(default)]
    pub embedded_bundle_root: Option<PathBuf>,
    pub remote_bundle_root: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransportConfig {
    #[serde(default)]
    pub gzip_responses: bool,
    #[serde(default = "default_response_prefix_policy")]
    pub response_prefix_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_true")]
    pub redact: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    #[serde(alias = "install_secret_env")]
    pub device_binding_secret_env: String,
    #[serde(default = "default_handoff_ttl")]
    pub handoff_ttl_seconds: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PaymentConfig {
    #[serde(default)]
    pub provider_url: String,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub api_addr: SocketAddr,
    pub asset_addr: SocketAddr,
    pub server_secret: Vec<u8>,
    pub config_path: PathBuf,
}

impl LoadedConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config_path = path.as_ref().to_path_buf();
        let text = fs::read_to_string(&config_path).map_err(|source| ConfigError::Read {
            path: config_path.clone(),
            source,
        })?;
        let mut config: Config = toml::from_str(&text)?;
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));
        resolve_paths(&mut config, base);

        let api_addr = parse_loopback(&config.api.bind, "api.bind")?;
        let asset_addr = parse_loopback(&config.assets.bind, "assets.bind")?;
        if config.assets.url_prefix.is_empty() || !config.assets.url_prefix.starts_with('/') {
            return Err(ConfigError::Invalid(
                "assets.url_prefix must start with /".into(),
            ));
        }
        if config.session.ttl_seconds == 0 {
            return Err(ConfigError::Invalid(
                "session.ttl_seconds must be positive".into(),
            ));
        }
        if config.account.handoff_ttl_seconds == 0 {
            return Err(ConfigError::Invalid(
                "account.handoff_ttl_seconds must be positive".into(),
            ));
        }
        if !config.logging.redact {
            return Err(ConfigError::Invalid(
                "logging.redact must remain true".into(),
            ));
        }
        if config.logging.format != "json" {
            return Err(ConfigError::Invalid("logging.format must be json".into()));
        }
        if !matches!(
            config.transport.response_prefix_policy.as_str(),
            "random" | "request"
        ) {
            return Err(ConfigError::Invalid(
                "transport.response_prefix_policy is invalid".into(),
            ));
        }
        if config.transport.gzip_responses {
            return Err(ConfigError::Invalid(
                "gzip_responses is not implemented in the initial slice".into(),
            ));
        }
        let secret_name = config.account.device_binding_secret_env.clone();
        let secret = env::var(&secret_name)
            .map_err(|_| ConfigError::Invalid(format!("missing {secret_name}")))?;
        if secret.is_empty() {
            return Err(ConfigError::Invalid(format!("{secret_name} is empty")));
        }
        Ok(Self {
            config,
            api_addr,
            asset_addr,
            server_secret: secret.into_bytes(),
            config_path,
        })
    }
}

fn resolve_paths(config: &mut Config, base: &Path) {
    for path in [
        &mut config.paths.protoset,
        &mut config.paths.master_data,
        &mut config.paths.master_data_download,
        &mut config.paths.manifest,
        &mut config.paths.catalog,
        &mut config.paths.asset_root,
        &mut config.paths.remote_bundle_root,
        &mut config.storage.path,
    ] {
        if path.is_relative() {
            *path = base.join(&*path);
        }
    }
    if let Some(path) = &mut config.paths.master_data_decoded {
        if path.is_relative() {
            *path = base.join(&*path);
        }
    }
    if let Some(path) = &mut config.paths.embedded_bundle_root {
        if path.is_relative() {
            *path = base.join(&*path);
        }
    }
}

fn parse_loopback(value: &str, field: &str) -> Result<SocketAddr, ConfigError> {
    let addr: SocketAddr = value
        .parse()
        .map_err(|_| ConfigError::Invalid(format!("{field} must be a socket address")))?;
    if !addr.ip().is_loopback() {
        return Err(ConfigError::Invalid(format!(
            "{field} must use a loopback address"
        )));
    }
    Ok(addr)
}

fn default_true() -> bool {
    true
}
fn default_response_prefix_policy() -> String {
    "random".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_format() -> String {
    "json".into()
}
fn default_handoff_ttl() -> u64 {
    600
}

#[cfg(test)]
mod tests {
    use super::parse_loopback;

    #[test]
    fn rejects_non_loopback_bind() {
        assert!(parse_loopback("0.0.0.0:18080", "api.bind").is_err());
    }
}
