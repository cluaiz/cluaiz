//! UI response parsing only. The client owns authorization and grant scope.

#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalResponse {
    Allow,
    Session,
    Deny,
    Feedback(String),
}

impl ApprovalResponse {
    /// Only explicit allow words grant consent; other text is preserved as feedback.
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        match input.to_ascii_lowercase().as_str() {
            "y" | "yes" => Self::Allow,
            "a" | "always" => Self::Session,
            "" | "n" | "no" => Self::Deny,
            _ => Self::Feedback(input.to_owned()),
        }
    }
}

pub struct Approval {
    pub title: String,
    pub detail: String,
}
