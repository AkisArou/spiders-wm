use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewDiagnostic {
    pub source: String,
    pub level: String,
    pub message: String,
}
