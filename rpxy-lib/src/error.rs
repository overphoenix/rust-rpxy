pub use anyhow::{anyhow, bail, ensure, Context};
use thiserror::Error;

pub type RpxyResult<T> = std::result::Result<T, RpxyError>;

/// Describes things that can go wrong in the Rpxy
#[derive(Debug, Error)]
pub enum RpxyError {
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),

  #[error("Certificate reload error: {0}")]
  CertificateReloadError(#[from] hot_reload::ReloaderError<crate::crypto::ServerCryptoBase>),

  // backend errors
  #[error("Invalid reverse proxy setting")]
  InvalidReverseProxyConfig,
  #[error("Invalid upstream option setting")]
  InvalidUpstreamOptionSetting,
  #[error("Failed to build backend app: {0}")]
  FailedToBuildBackendApp(#[from] crate::backend::BackendAppBuilderError),

  #[error("Unsupported upstream option")]
  UnsupportedUpstreamOption,
}
