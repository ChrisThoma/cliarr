use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliarrError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("{service}: authentication failed; check the API key/credentials in your config")]
    Auth { service: &'static str },

    #[error("{service} returned HTTP {status}: {body}")]
    Api {
        service: &'static str,
        status: u16,
        body: String,
    },

    #[error("config error: {0}")]
    Config(String),

    #[error("{0} is not configured; run `cliarr config init` or edit the config file")]
    NotConfigured(&'static str),

    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),

    #[error("{0}")]
    Other(String),
}

impl CliarrError {
    /// Process exit code: 2 for config/usage problems, 1 for everything else.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliarrError::Config(_) | CliarrError::NotConfigured(_) => 2,
            _ => 1,
        }
    }
}

pub type Result<T, E = CliarrError> = std::result::Result<T, E>;
