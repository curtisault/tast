/// Data extracted from step prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedData {
    /// Key-value pairs extracted from the text.
    pub fields: Vec<(String, String)>,
}

/// Binding verbs that signal a key-value relationship follows.
const BINDING_VERBS: &[&str] = &["is", "has", "with", "having", "contains"];

/// Extract structured data from step text.
///
/// Scans for patterns like:
/// - `email "foo@bar.com"` -> `("email", "foo@bar.com")`
/// - `with email "foo@bar.com"` -> `("email", "foo@bar.com")`
/// - `age 25` -> `("age", "25")`
/// - `status "active"` -> `("status", "active")`
///
/// Binding verbs (`is`, `has`, `with`, `having`, `contains`) are consumed
/// as connectors — the word *after* the verb becomes the key.
pub fn extract_data(text: &str) -> ExtractedData {
    let tokens = tokenize_for_extraction(text);
    let mut fields = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            ExtractToken::BindingVerb => {
                // binding_verb <key> <quoted_value|number>
                if i + 2 < tokens.len()
                    && let ExtractToken::Word(key) = &tokens[i + 1]
                {
                    match &tokens[i + 2] {
                        ExtractToken::QuotedString(val) => {
                            fields.push((key.clone(), val.clone()));
                            i += 3;
                            continue;
                        }
                        ExtractToken::Number(val) => {
                            fields.push((key.clone(), val.clone()));
                            i += 3;
                            continue;
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
            ExtractToken::Word(key) => {
                if i + 1 < tokens.len() {
                    // <word> <quoted_value|number>
                    match &tokens[i + 1] {
                        ExtractToken::QuotedString(val) => {
                            fields.push((key.clone(), val.clone()));
                            i += 2;
                            continue;
                        }
                        ExtractToken::Number(val) => {
                            fields.push((key.clone(), val.clone()));
                            i += 2;
                            continue;
                        }
                        // <word> <binding_verb> <quoted_value|number>
                        // e.g., "status is "active""
                        ExtractToken::BindingVerb => {
                            if i + 2 < tokens.len() {
                                match &tokens[i + 2] {
                                    ExtractToken::QuotedString(val) => {
                                        fields.push((key.clone(), val.clone()));
                                        i += 3;
                                        continue;
                                    }
                                    ExtractToken::Number(val) => {
                                        fields.push((key.clone(), val.clone()));
                                        i += 3;
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    ExtractedData { fields }
}

#[derive(Debug)]
enum ExtractToken {
    Word(String),
    QuotedString(String),
    Number(String),
    BindingVerb,
}

/// Tokenize text for data extraction purposes.
fn tokenize_for_extraction(text: &str) -> Vec<ExtractToken> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        if chars.peek().is_none() {
            break;
        }

        // Quoted string
        if chars.peek() == Some(&'"') {
            chars.next(); // skip opening quote
            let mut s = String::new();
            loop {
                match chars.next() {
                    Some('\\') => {
                        if let Some(escaped) = chars.next() {
                            s.push(escaped);
                        }
                    }
                    Some('"') => break,
                    Some(c) => s.push(c),
                    None => break,
                }
            }
            tokens.push(ExtractToken::QuotedString(s));
            continue;
        }

        // Collect a word or number
        let mut word = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '"' {
                break;
            }
            word.push(c);
            chars.next();
        }

        if word.is_empty() {
            continue;
        }

        // Check if it's a number (integer or decimal)
        if word.chars().all(|c| c.is_ascii_digit() || c == '.')
            && word.contains(|c: char| c.is_ascii_digit())
        {
            tokens.push(ExtractToken::Number(word));
        } else {
            let lower = word.to_lowercase();
            if BINDING_VERBS.contains(&lower.as_str()) {
                tokens.push(ExtractToken::BindingVerb);
            } else {
                tokens.push(ExtractToken::Word(lower));
            }
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_quoted_string_after_word() {
        let result = extract_data(r#"email "foo@bar.com""#);
        assert_eq!(result.fields, vec![("email".into(), "foo@bar.com".into())]);
    }

    #[test]
    fn extracts_multiple_quoted_values() {
        let result = extract_data(r#"email "foo@bar.com" password "secret""#);
        assert_eq!(result.fields.len(), 2);
        assert_eq!(result.fields[0], ("email".into(), "foo@bar.com".into()));
        assert_eq!(result.fields[1], ("password".into(), "secret".into()));
    }

    #[test]
    fn extracts_number_after_word() {
        let result = extract_data("age 25");
        assert_eq!(result.fields, vec![("age".into(), "25".into())]);
    }

    #[test]
    fn extracts_after_binding_verb_with() {
        let result = extract_data(r#"user with email "test@example.com""#);
        assert_eq!(
            result.fields,
            vec![("email".into(), "test@example.com".into())]
        );
    }

    #[test]
    fn extracts_after_binding_verb_has() {
        let result = extract_data(r#"user has email "test@example.com""#);
        assert_eq!(
            result.fields,
            vec![("email".into(), "test@example.com".into())]
        );
    }

    #[test]
    fn extracts_after_binding_verb_is() {
        let result = extract_data(r#"status is "active""#);
        assert_eq!(result.fields, vec![("status".into(), "active".into())]);
    }

    #[test]
    fn extracts_nothing_from_plain_text() {
        let result = extract_data("the user submits the form");
        assert!(result.fields.is_empty());
    }

    #[test]
    fn extracts_preserves_key_name() {
        let result = extract_data(r#"userName "alice""#);
        assert_eq!(result.fields[0].0, "username"); // lowercased
    }

    #[test]
    fn extracts_handles_multiple_data_points() {
        let result = extract_data(r#"with email "a@b.com" and with role "admin""#);
        assert_eq!(result.fields.len(), 2);
        assert_eq!(result.fields[0], ("email".into(), "a@b.com".into()));
        assert_eq!(result.fields[1], ("role".into(), "admin".into()));
    }

    #[test]
    fn extracts_ignores_quoted_strings_without_preceding_key() {
        // A quoted string at the very start with no preceding word
        let result = extract_data(r#""orphan_value""#);
        assert!(result.fields.is_empty());
    }

    #[test]
    fn extracts_email_pattern() {
        let result = extract_data(r#"a user with email "test@example.com""#);
        assert_eq!(
            result.fields,
            vec![("email".into(), "test@example.com".into())]
        );
    }

    #[test]
    fn extracts_compound_pattern() {
        let result = extract_data(r#"a user with email "x@y.com" and password "secret123""#);
        assert_eq!(result.fields.len(), 2);
        assert_eq!(result.fields[0], ("email".into(), "x@y.com".into()));
        assert_eq!(result.fields[1], ("password".into(), "secret123".into()));
    }

    #[test]
    fn extracts_url_pattern() {
        let result = extract_data(r#"the user navigates to "/dashboard""#);
        assert_eq!(result.fields, vec![("to".into(), "/dashboard".into())]);
    }

    #[test]
    fn extracts_does_not_extract_from_data_block_text() {
        // Plain text without any quoted strings or numbers — nothing to extract
        let result = extract_data("a user with valid credentials");
        assert!(result.fields.is_empty());
    }
}
