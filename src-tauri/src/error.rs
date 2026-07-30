use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("vault is not initialized")]
    NotInitialized,
    #[error("vault is already initialized")]
    AlreadyInitialized,
    #[error("vault is locked")]
    Locked,
    #[error("vault is already unlocked")]
    AlreadyUnlocked,
    #[error("incorrect password or recovery key")]
    BadPassword,
    #[error("item not found")]
    NotFound,
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("file too large (max {0} bytes)")]
    TooLarge(u64),
    #[error("invalid name")]
    InvalidName,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::Io(value.to_string())
    }
}
