/// Capitalize the first character of a string.
pub(crate) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalizes_lowercase() {
        assert_eq!(capitalize("given"), "Given");
    }

    #[test]
    fn capitalizes_empty() {
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn capitalizes_already_upper() {
        assert_eq!(capitalize("When"), "When");
    }

    #[test]
    fn capitalizes_single_char() {
        assert_eq!(capitalize("a"), "A");
    }
}
