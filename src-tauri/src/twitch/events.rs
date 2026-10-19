use serde::Serialize;

/// Serialized as `{"state":"connected"}` / `{"state":"error","message":"…"}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(tag = "state", content = "message", rename_all = "snake_case")]
pub enum ConnStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error(String),
}

impl ConnStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Disconnected => "Disconnected".into(),
            Self::Connecting => "Connecting".into(),
            Self::Connected => "Connected".into(),
            Self::Reconnecting => "Reconnecting".into(),
            Self::Error(e) => format!("Error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_for_the_frontend() {
        assert_eq!(
            serde_json::to_string(&ConnStatus::Connected).unwrap(),
            r#"{"state":"connected"}"#
        );
        assert_eq!(
            serde_json::to_string(&ConnStatus::Error("x".into())).unwrap(),
            r#"{"state":"error","message":"x"}"#
        );
    }
}
