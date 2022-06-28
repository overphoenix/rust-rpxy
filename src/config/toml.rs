use crate::error::*;
use serde::Deserialize;
use std::{collections::HashMap, fs};

#[derive(Deserialize, Debug, Default)]
pub struct ConfigToml {
  pub listen_port: Option<u16>,
  pub listen_port_tls: Option<u16>,
  pub listen_ipv6: Option<bool>,
  pub max_concurrent_streams: Option<u32>,
  pub max_clients: Option<u32>,
  pub apps: Option<Apps>,
  pub default_app: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct Apps(pub HashMap<String, Application>);

#[derive(Deserialize, Debug, Default)]
pub struct Application {
  pub server_name: Option<String>,
  pub reverse_proxy: Option<Vec<ReverseProxyOption>>,
  pub tls: Option<TlsOption>,
}

#[derive(Deserialize, Debug, Default)]
pub struct TlsOption {
  pub tls_cert_path: Option<String>,
  pub tls_cert_key_path: Option<String>,
  pub https_redirection: Option<bool>,
}

#[derive(Deserialize, Debug, Default)]
pub struct ReverseProxyOption {
  pub path: Option<String>,
  pub upstream: Vec<UpstreamOption>,
}

#[derive(Deserialize, Debug, Default)]
pub struct UpstreamOption {
  pub location: String,
  pub tls: Option<bool>,
}
impl UpstreamOption {
  pub fn to_uri(&self) -> Result<hyper::Uri> {
    let mut scheme = "http";
    if let Some(t) = self.tls {
      if t {
        scheme = "https";
      }
    }
    let location = format!("{}://{}", scheme, self.location);
    location.parse::<hyper::Uri>().map_err(|e| anyhow!("{}", e))
  }
}

impl ConfigToml {
  pub fn new(config_file: &str) -> Result<Self> {
    let config_str = if let Ok(s) = fs::read_to_string(config_file) {
      s
    } else {
      bail!("Failed to read config file");
    };
    let parsed: Result<ConfigToml> = toml::from_str(&config_str)
      .map_err(|e: toml::de::Error| anyhow!("Failed to parse toml config: {:?}", e));
    parsed
  }
}
