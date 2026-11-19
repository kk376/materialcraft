#[derive(Debug)]
pub(super) enum EmbeddedError {
    Damaged(String),
    Budget,
    WorkBudget,
}

impl From<String> for EmbeddedError {
    fn from(message: String) -> Self {
        Self::Damaged(message)
    }
}

impl From<&str> for EmbeddedError {
    fn from(message: &str) -> Self {
        Self::Damaged(message.into())
    }
}

impl std::fmt::Display for EmbeddedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Damaged(message) => formatter.write_str(message),
            Self::Budget => formatter.write_str("embedded resource exceeds the remaining material resource budget"),
            Self::WorkBudget => formatter.write_str("embedded resource exceeds the material inflation work budget"),
        }
    }
}
