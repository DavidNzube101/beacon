use thiserror::Error;
use serde::Serialize;

#[derive(Error, Debug)]
pub enum BeaconError {
    #[error("Scanning failed: {0}")]
    ScanError(String),

    #[error("Inference failed: {0}")]
    InferenceError(String),

    #[error("Validation failed: {0}")]
    ValidationError(String),

    #[error("Payment required to proceed")]
    PaymentRequired {
        run_id: String,
        amount: String,
        base_addr: String,
        sol_addr: String,
    },

    #[error("Beacon Cloud returned an error: {status} - {message}")]
    CloudError {
        status: u16,
        message: String,
    },

    #[error("Failed to parse response from Beacon Cloud: {0}")]
    ParseError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: String,
}
