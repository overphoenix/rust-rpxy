mod upstream;
mod upstream_opts;

use crate::log::*;
use rustc_hash::FxHashMap as HashMap;
use std::{
  fs::File,
  io::{self, BufReader, Cursor, Read},
  path::PathBuf,
  sync::Arc,
};
use tokio_rustls::rustls::{
  server::ResolvesServerCertUsingSni,
  sign::{any_supported_type, CertifiedKey},
  Certificate, PrivateKey, ServerConfig,
};
pub use upstream::{ReverseProxy, Upstream, UpstreamGroup};
pub use upstream_opts::UpstreamOption;

// Server name (hostname or ip address) and path name representation in backends
// For searching hashmap or key list by exact or longest-prefix matching
pub type ServerNameBytesExp = Vec<u8>; // lowercase ascii bytes
pub type PathNameBytesExp = Vec<u8>; // lowercase ascii bytes

/// Struct serving information to route incoming connections, like server name to be handled and tls certs/keys settings.
pub struct Backend {
  pub app_name: String,
  pub server_name: String,
  pub reverse_proxy: ReverseProxy,

  // tls settings
  pub tls_cert_path: Option<PathBuf>,
  pub tls_cert_key_path: Option<PathBuf>,
  pub https_redirection: Option<bool>,
}

impl Backend {
  pub fn read_certs_and_key(&self) -> io::Result<CertifiedKey> {
    debug!("Read TLS server certificates and private key");
    let (certs_path, certs_keys_path) =
      if let (Some(c), Some(k)) = (self.tls_cert_path.as_ref(), self.tls_cert_key_path.as_ref()) {
        (c, k)
      } else {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid certs and keys paths"));
      };
    let certs: Vec<_> = {
      let certs_path_str = certs_path.display().to_string();
      let mut reader = BufReader::new(File::open(certs_path).map_err(|e| {
        io::Error::new(
          e.kind(),
          format!("Unable to load the certificates [{}]: {}", certs_path_str, e),
        )
      })?);
      rustls_pemfile::certs(&mut reader)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Unable to parse the certificates"))?
    }
    .drain(..)
    .map(Certificate)
    .collect();
    let certs_keys: Vec<_> = {
      let certs_keys_path_str = certs_keys_path.display().to_string();
      let encoded_keys = {
        let mut encoded_keys = vec![];
        File::open(certs_keys_path)
          .map_err(|e| {
            io::Error::new(
              e.kind(),
              format!("Unable to load the certificate keys [{}]: {}", certs_keys_path_str, e),
            )
          })?
          .read_to_end(&mut encoded_keys)?;
        encoded_keys
      };
      let mut reader = Cursor::new(encoded_keys);
      let pkcs8_keys = rustls_pemfile::pkcs8_private_keys(&mut reader).map_err(|_| {
        io::Error::new(
          io::ErrorKind::InvalidInput,
          "Unable to parse the certificates private keys (PKCS8)",
        )
      })?;
      reader.set_position(0);
      let mut rsa_keys = rustls_pemfile::rsa_private_keys(&mut reader)?;
      let mut keys = pkcs8_keys;
      keys.append(&mut rsa_keys);
      if keys.is_empty() {
        return Err(io::Error::new(
          io::ErrorKind::InvalidInput,
          "No private keys found - Make sure that they are in PKCS#8/PEM format",
        ));
      }
      keys.drain(..).map(PrivateKey).collect()
    };
    let signing_key = certs_keys
      .iter()
      .find_map(|k| {
        if let Ok(sk) = any_supported_type(k) {
          Some(sk)
        } else {
          None
        }
      })
      .ok_or_else(|| {
        io::Error::new(
          io::ErrorKind::InvalidInput,
          "Unable to find a valid certificate and key",
        )
      })?;
    Ok(CertifiedKey::new(certs, signing_key))
  }
}

/// HashMap and some meta information for multiple Backend structs.
pub struct Backends {
  pub apps: HashMap<ServerNameBytesExp, Backend>, // hyper::uriで抜いたhostで引っ掛ける
  pub default_server_name_bytes: Option<ServerNameBytesExp>, // for plaintext http
}

impl Backends {
  pub async fn generate_server_crypto_with_cert_resolver(&self) -> Result<ServerConfig, anyhow::Error> {
    let mut resolver = ResolvesServerCertUsingSni::new();

    // let mut cnt = 0;
    for (_, backend) in self.apps.iter() {
      if backend.tls_cert_key_path.is_some() && backend.tls_cert_path.is_some() {
        match backend.read_certs_and_key() {
          Ok(certified_key) => {
            if let Err(e) = resolver.add(backend.server_name.as_str(), certified_key) {
              error!(
                "{}: Failed to read some certificates and keys {}",
                backend.server_name.as_str(),
                e
              )
            } else {
              // debug!("Add certificate for server_name: {}", backend.server_name.as_str());
              // cnt += 1;
            }
          }
          Err(e) => {
            warn!("Failed to add certificate for {}: {}", backend.server_name.as_str(), e);
          }
        }
      }
    }
    // debug!("Load certificate chain for {} server_name's", cnt);

    let mut server_config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_cert_resolver(Arc::new(resolver));

    #[cfg(feature = "http3")]
    {
      server_config.alpn_protocols = vec![
        b"h3".to_vec(),
        b"hq-29".to_vec(), // TODO: remove later?
        b"h2".to_vec(),
        b"http/1.1".to_vec(),
      ];
    }
    #[cfg(not(feature = "http3"))]
    {
      server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    }

    Ok(server_config)
  }
}
