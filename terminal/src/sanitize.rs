//! The renderer owns terminal escape sequences. Untrusted text contributes printable cells only.

pub(crate) fn display_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
