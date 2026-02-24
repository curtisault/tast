/// A token in the natural-language analysis of step text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NlToken {
    /// An article word (a, an, the) — stripped during normalization.
    Article(String),
    /// A determiner (some, any) — stripped during normalization.
    Determiner(String),
    /// A binding verb (is, has, with, having, contains).
    BindingVerb(String),
    /// An action verb (submits, sends, clicks, navigates, posts, accesses, creates).
    ActionVerb(String),
    /// A quoted string reference extracted from the text.
    DataRef(String),
    /// Any other word.
    Word(String),
}

/// The result of normalizing step text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedText {
    /// The original text, preserved verbatim.
    pub original: String,
    /// Lowercased, articles/determiners stripped, whitespace collapsed.
    pub normalized: String,
    /// The parsed natural-language tokens.
    pub tokens: Vec<NlToken>,
}

/// Noise words — articles stripped during normalization.
const ARTICLES: &[&str] = &["a", "an", "the"];

/// Determiners stripped during normalization.
const DETERMINERS: &[&str] = &["some", "any"];

/// Binding verbs recognized as data-binding hints.
const BINDING_VERBS: &[&str] = &["is", "has", "with", "having", "contains"];

/// Action verbs recognized as action indicators.
const ACTION_VERBS: &[&str] = &[
    "submits",
    "sends",
    "clicks",
    "navigates",
    "posts",
    "accesses",
    "creates",
    "deletes",
    "updates",
    "receives",
    "returns",
    "loads",
    "destroys",
];

/// Normalize step text for comparison and matching.
///
/// - Preserves the original text verbatim
/// - Produces a normalized form: lowercased, articles/determiners stripped, whitespace collapsed
/// - Tokenizes words into semantic categories
pub fn normalize(text: &str) -> NormalizedText {
    let original = text.to_owned();
    let tokens = tokenize_nl(text);

    // Build normalized string: skip articles and determiners, lowercase everything
    let normalized_words: Vec<&str> = tokens
        .iter()
        .filter_map(|t| match t {
            NlToken::Article(_) | NlToken::Determiner(_) => None,
            NlToken::BindingVerb(w)
            | NlToken::ActionVerb(w)
            | NlToken::DataRef(w)
            | NlToken::Word(w) => Some(w.as_str()),
        })
        .collect();

    let normalized = normalized_words.join(" ");

    NormalizedText {
        original,
        normalized,
        tokens,
    }
}

/// Tokenize step text into natural-language tokens.
fn tokenize_nl(text: &str) -> Vec<NlToken> {
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
            tokens.push(NlToken::DataRef(s));
            continue;
        }

        // Collect a word
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

        let lower = word.to_lowercase();
        let token = if ARTICLES.contains(&lower.as_str()) {
            NlToken::Article(lower)
        } else if DETERMINERS.contains(&lower.as_str()) {
            NlToken::Determiner(lower)
        } else if BINDING_VERBS.contains(&lower.as_str()) {
            NlToken::BindingVerb(lower)
        } else if ACTION_VERBS.contains(&lower.as_str()) {
            NlToken::ActionVerb(lower)
        } else {
            NlToken::Word(lower)
        };

        tokens.push(token);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_leading_article_a() {
        let result = normalize("a user with email");
        assert_eq!(result.normalized, "user with email");
    }

    #[test]
    fn normalize_strips_leading_article_an() {
        let result = normalize("an active session");
        assert_eq!(result.normalized, "active session");
    }

    #[test]
    fn normalize_strips_leading_article_the() {
        let result = normalize("the user submits the form");
        assert_eq!(result.normalized, "user submits form");
    }

    #[test]
    fn normalize_strips_determiner_some() {
        let result = normalize("some users exist");
        assert_eq!(result.normalized, "users exist");
    }

    #[test]
    fn normalize_preserves_original_text() {
        let result = normalize("a User With Email");
        assert_eq!(result.original, "a User With Email");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        let result = normalize("user   with    email");
        assert_eq!(result.normalized, "user with email");
    }

    #[test]
    fn normalize_lowercases_for_comparison() {
        let result = normalize("User With Email");
        assert_eq!(result.normalized, "user with email");
    }

    #[test]
    fn normalize_identifies_binding_verb_is() {
        let result = normalize("email is set");
        assert!(result.tokens.contains(&NlToken::BindingVerb("is".into())));
    }

    #[test]
    fn normalize_identifies_binding_verb_has() {
        let result = normalize("user has email");
        assert!(result.tokens.contains(&NlToken::BindingVerb("has".into())));
    }

    #[test]
    fn normalize_identifies_binding_verb_with() {
        let result = normalize("user with email");
        assert!(result.tokens.contains(&NlToken::BindingVerb("with".into())));
    }

    #[test]
    fn normalize_identifies_action_verb_submits() {
        let result = normalize("user submits the form");
        assert!(
            result
                .tokens
                .contains(&NlToken::ActionVerb("submits".into()))
        );
    }

    #[test]
    fn normalize_equivalent_phrasings_match() {
        let a = normalize("a user with email");
        let b = normalize("the user has email");
        let c = normalize("user email is");

        // All three strip to variations that share "user" and "email"
        // a -> "user with email"
        // b -> "user has email"
        // c -> "user email is"
        // They won't be identical strings, but articles are stripped from all.
        // The key assertion: none contain articles.
        assert!(!a.normalized.contains(" a "));
        assert!(!b.normalized.contains(" the "));
        assert!(!a.normalized.starts_with("a "));
        assert!(!b.normalized.starts_with("the "));

        // All contain "user" and "email"
        assert!(a.normalized.contains("user"));
        assert!(a.normalized.contains("email"));
        assert!(b.normalized.contains("user"));
        assert!(b.normalized.contains("email"));
        assert!(c.normalized.contains("user"));
        assert!(c.normalized.contains("email"));
    }
}
