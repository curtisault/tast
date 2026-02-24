use crate::util::span::Span;

/// A token kind produced by the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenKind {
    // Keywords
    Graph,
    Node,
    Describe,
    Given,
    When,
    Then,
    And,
    But,
    Passes,
    Requires,
    Tags,
    Config,
    Import,
    Fixture,
    From,

    // Symbols
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Arrow,
    Colon,
    Comma,
    Dot,

    // Literals & identifiers
    StringLiteral(String),
    Identifier(String),

    /// Free-form text after step keywords (given/when/then/and/but).
    FreeText(String),

    /// A `<name>` parameter placeholder inside step text.
    Parameter(String),

    /// A `# comment` line.
    Comment(String),

    Newline,
}

/// A token with its kind and source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// An error encountered during lexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.message)
    }
}

impl std::error::Error for LexError {}

/// Tokenizes a `.tast` source string into a vector of tokens.
///
/// # Errors
///
/// Returns a [`LexError`] if the input contains invalid syntax,
/// such as an unterminated string literal.
pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    let mut line: usize = 1;
    let mut col: usize = 1;

    while let Some(&(pos, ch)) = chars.peek() {
        match ch {
            // Newlines
            '\n' => {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                line += 1;
                col = 1;
            }

            // Skip whitespace (not newlines)
            ' ' | '\t' | '\r' => {
                chars.next();
                col += 1;
            }

            // Comments
            '#' => {
                let start_col = col;
                chars.next(); // skip '#'
                // skip optional space after #
                if let Some(&(_, ' ')) = chars.peek() {
                    chars.next();
                    col += 1;
                }
                col += 1; // for the '#'

                let comment_start = chars.peek().map_or(input.len(), |&(i, _)| i);
                let mut end = comment_start;
                while let Some(&(i, c)) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    end = i + c.len_utf8();
                    chars.next();
                    col += 1;
                }
                let text = &input[comment_start..end];
                tokens.push(Token {
                    kind: TokenKind::Comment(text.to_owned()),
                    span: Span::new(pos, end, line, start_col),
                });
            }

            // String literals
            '"' => {
                let start_col = col;
                chars.next(); // skip opening quote
                col += 1;
                let mut s = String::new();
                let mut terminated = false;
                while let Some(&(_, c)) = chars.peek() {
                    chars.next();
                    col += 1;
                    if c == '\\' {
                        if let Some(&(_, escaped)) = chars.peek() {
                            chars.next();
                            col += 1;
                            match escaped {
                                '"' => s.push('"'),
                                '\\' => s.push('\\'),
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                other => {
                                    s.push('\\');
                                    s.push(other);
                                }
                            }
                        }
                    } else if c == '"' {
                        terminated = true;
                        break;
                    } else {
                        s.push(c);
                    }
                }
                if !terminated {
                    return Err(LexError {
                        message: "unterminated string literal".to_owned(),
                        span: Span::new(pos, input.len(), line, start_col),
                    });
                }
                let end_pos = chars.peek().map_or(input.len(), |&(i, _)| i);
                tokens.push(Token {
                    kind: TokenKind::StringLiteral(s),
                    span: Span::new(pos, end_pos, line, start_col),
                });
            }

            // Symbols
            '{' => {
                tokens.push(Token {
                    kind: TokenKind::LBrace,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }
            '}' => {
                tokens.push(Token {
                    kind: TokenKind::RBrace,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }
            '[' => {
                tokens.push(Token {
                    kind: TokenKind::LBracket,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }
            ']' => {
                tokens.push(Token {
                    kind: TokenKind::RBracket,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }
            ':' => {
                tokens.push(Token {
                    kind: TokenKind::Colon,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }
            ',' => {
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }
            '.' => {
                tokens.push(Token {
                    kind: TokenKind::Dot,
                    span: Span::new(pos, pos + 1, line, col),
                });
                chars.next();
                col += 1;
            }

            // Arrow ->
            '-' => {
                chars.next();
                col += 1;
                if let Some(&(_, '>')) = chars.peek() {
                    chars.next();
                    tokens.push(Token {
                        kind: TokenKind::Arrow,
                        span: Span::new(pos, pos + 2, line, col - 1),
                    });
                    col += 1;
                } else {
                    // Stray '-' — treat as start of free text or error.
                    // For Phase 1 strict mode, we'll just skip it.
                    // This will be handled better when we have full error recovery.
                }
            }

            // Identifiers and keywords
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start_col = col;
                let start = pos;
                let mut end = pos;
                while let Some(&(i, c)) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        end = i + c.len_utf8();
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let word = &input[start..end];
                let is_step_keyword = matches!(word, "given" | "when" | "then" | "and" | "but");

                let kind = match word {
                    "graph" => TokenKind::Graph,
                    "node" => TokenKind::Node,
                    "describe" => TokenKind::Describe,
                    "given" => TokenKind::Given,
                    "when" => TokenKind::When,
                    "then" => TokenKind::Then,
                    "and" => TokenKind::And,
                    "but" => TokenKind::But,
                    "passes" => TokenKind::Passes,
                    "requires" => TokenKind::Requires,
                    "tags" => TokenKind::Tags,
                    "config" => TokenKind::Config,
                    "import" => TokenKind::Import,
                    "fixture" => TokenKind::Fixture,
                    "from" => TokenKind::From,
                    _ => TokenKind::Identifier(word.to_owned()),
                };

                tokens.push(Token {
                    kind,
                    span: Span::new(start, end, line, start_col),
                });

                // After step keywords, consume free text until end of line or data block.
                // Detect `<name>` parameter placeholders and emit them as separate tokens.
                if is_step_keyword {
                    // Skip whitespace (not newlines)
                    while let Some(&(_, c)) = chars.peek() {
                        if c == ' ' || c == '\t' {
                            chars.next();
                            col += 1;
                        } else {
                            break;
                        }
                    }

                    // Collect free text, splitting on <param> patterns
                    let mut text_buf = String::new();
                    let text_start = chars.peek().map_or(input.len(), |&(i, _)| i);
                    let _text_start_col = col;
                    let mut seg_start = text_start;
                    let mut seg_start_col = col;

                    while let Some(&(i, c)) = chars.peek() {
                        if c == '\n' || c == '{' {
                            break;
                        }

                        if c == '<' {
                            // Try to read a parameter: <identifier>
                            let angle_pos = i;
                            let angle_col = col;
                            chars.next();
                            col += 1;

                            let mut param_name = String::new();
                            let mut valid = false;
                            while let Some(&(_, pc)) = chars.peek() {
                                if pc == '>' {
                                    chars.next();
                                    col += 1;
                                    valid = !param_name.is_empty();
                                    break;
                                }
                                if pc == '\n' || pc == '{' || pc == '<' {
                                    break;
                                }
                                if pc.is_ascii_alphanumeric() || pc == '_' {
                                    param_name.push(pc);
                                    chars.next();
                                    col += 1;
                                } else {
                                    break;
                                }
                            }

                            if valid {
                                // Flush accumulated text before the parameter
                                let trimmed = text_buf.trim();
                                if !trimmed.is_empty() {
                                    tokens.push(Token {
                                        kind: TokenKind::FreeText(trimmed.to_owned()),
                                        span: Span::new(seg_start, angle_pos, line, seg_start_col),
                                    });
                                }
                                text_buf.clear();

                                // Emit the parameter token
                                tokens.push(Token {
                                    kind: TokenKind::Parameter(param_name),
                                    span: Span::new(
                                        angle_pos,
                                        angle_pos + col - angle_col,
                                        line,
                                        angle_col,
                                    ),
                                });

                                // Update segment start for next text
                                seg_start = chars.peek().map_or(input.len(), |&(idx, _)| idx);
                                seg_start_col = col;
                            } else {
                                // Not a valid parameter — treat '<' and consumed chars as text
                                text_buf.push('<');
                                text_buf.push_str(&param_name);
                                // If we stopped at '>', we already consumed it
                                // Otherwise the char is still in the iterator
                            }
                        } else {
                            text_buf.push(c);
                            chars.next();
                            col += 1;
                        }
                    }

                    // Flush remaining text
                    let trimmed = text_buf.trim_end();
                    if !trimmed.is_empty() {
                        let end = chars.peek().map_or(input.len(), |&(idx, _)| idx);
                        tokens.push(Token {
                            kind: TokenKind::FreeText(trimmed.to_owned()),
                            span: Span::new(seg_start, end, line, seg_start_col),
                        });
                    }
                }
            }

            _ => {
                // Skip unrecognized characters
                chars.next();
                col += 1;
            }
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: collect just the token kinds from input, ignoring spans.
    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .expect("unexpected lex error")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    /// Helper: collect token kinds, filtering out newlines.
    fn kinds_no_newlines(input: &str) -> Vec<TokenKind> {
        kinds(input)
            .into_iter()
            .filter(|k| *k != TokenKind::Newline)
            .collect()
    }

    #[test]
    fn tokenizes_empty_input() {
        let tokens = tokenize("").expect("should succeed");
        assert!(tokens.is_empty());
    }

    #[test]
    fn tokenizes_single_keyword_graph() {
        assert_eq!(kinds("graph"), vec![TokenKind::Graph]);
    }

    #[test]
    fn tokenizes_single_keyword_node() {
        assert_eq!(kinds("node"), vec![TokenKind::Node]);
    }

    #[test]
    fn tokenizes_all_keywords() {
        let input = "graph node describe given when then and but passes requires tags config import fixture from";
        let result = kinds(input);
        // Step keywords (given/when/then/and/but) consume trailing free text,
        // so subsequent keywords get captured as free text.
        // Test them individually instead.
        assert_eq!(result[0], TokenKind::Graph);
        assert_eq!(result[1], TokenKind::Node);
        assert_eq!(result[2], TokenKind::Describe);

        // Test step keywords in isolation
        assert_eq!(kinds("given"), vec![TokenKind::Given]);
        assert_eq!(kinds("when"), vec![TokenKind::When]);
        assert_eq!(kinds("then"), vec![TokenKind::Then]);
        assert_eq!(kinds("and"), vec![TokenKind::And]);
        assert_eq!(kinds("but"), vec![TokenKind::But]);

        // Non-step keywords
        assert_eq!(kinds("passes"), vec![TokenKind::Passes]);
        assert_eq!(kinds("requires"), vec![TokenKind::Requires]);
        assert_eq!(kinds("tags"), vec![TokenKind::Tags]);
        assert_eq!(kinds("config"), vec![TokenKind::Config]);
        assert_eq!(kinds("import"), vec![TokenKind::Import]);
        assert_eq!(kinds("fixture"), vec![TokenKind::Fixture]);
        assert_eq!(kinds("from"), vec![TokenKind::From]);
    }

    #[test]
    fn tokenizes_arrow_operator() {
        assert_eq!(kinds("->"), vec![TokenKind::Arrow]);
    }

    #[test]
    fn tokenizes_braces_and_brackets() {
        assert_eq!(
            kinds("{ } [ ]"),
            vec![
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
            ]
        );
    }

    #[test]
    fn tokenizes_string_literal() {
        assert_eq!(
            kinds(r#""hello world""#),
            vec![TokenKind::StringLiteral("hello world".into())]
        );
    }

    #[test]
    fn tokenizes_string_with_escaped_quotes() {
        assert_eq!(
            kinds(r#""he said \"hi\"""#),
            vec![TokenKind::StringLiteral("he said \"hi\"".into())]
        );
    }

    #[test]
    fn tokenizes_identifier_simple() {
        assert_eq!(
            kinds("UserAuth"),
            vec![TokenKind::Identifier("UserAuth".into())]
        );
    }

    #[test]
    fn tokenizes_identifier_with_underscores() {
        assert_eq!(
            kinds("login_user"),
            vec![TokenKind::Identifier("login_user".into())]
        );
    }

    #[test]
    fn tokenizes_identifier_with_dots() {
        // Auth.Login should tokenize as Identifier("Auth"), Dot, Identifier("Login")
        assert_eq!(
            kinds("Auth.Login"),
            vec![
                TokenKind::Identifier("Auth".into()),
                TokenKind::Dot,
                TokenKind::Identifier("Login".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_comment_line() {
        let tokens = kinds("# this is a comment");
        assert_eq!(tokens, vec![TokenKind::Comment("this is a comment".into())]);
    }

    #[test]
    fn tokenizes_comment_ignores_content() {
        let tokens = kinds("# graph node { given something");
        assert_eq!(
            tokens,
            vec![TokenKind::Comment("graph node { given something".into())]
        );
    }

    #[test]
    fn tokenizes_free_text_after_given() {
        let tokens = kinds("given a user with email");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Given,
                TokenKind::FreeText("a user with email".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_free_text_after_when() {
        let tokens = kinds("when the user submits the form");
        assert_eq!(
            tokens,
            vec![
                TokenKind::When,
                TokenKind::FreeText("the user submits the form".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_free_text_after_then() {
        let tokens = kinds("then the system creates a new account");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Then,
                TokenKind::FreeText("the system creates a new account".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_inline_data_block() {
        let tokens = kinds_no_newlines("{ email: \"test@example.com\" }");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBrace,
                TokenKind::Identifier("email".into()),
                TokenKind::Colon,
                TokenKind::StringLiteral("test@example.com".into()),
                TokenKind::RBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_colon_in_data_block() {
        let tokens = kinds_no_newlines("key: value");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Identifier("key".into()),
                TokenKind::Colon,
                TokenKind::Identifier("value".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_comma_in_data_block() {
        let tokens = kinds_no_newlines("{ a, b, c }");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBrace,
                TokenKind::Identifier("a".into()),
                TokenKind::Comma,
                TokenKind::Identifier("b".into()),
                TokenKind::Comma,
                TokenKind::Identifier("c".into()),
                TokenKind::RBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_complete_node_block() {
        let input = r#"node RegisterUser {
    describe "A new user registers"
    given a user with valid credentials
    when the user submits the registration form
    then the system creates a new account
}"#;
        let tokens = kinds_no_newlines(input);
        assert_eq!(tokens[0], TokenKind::Node);
        assert_eq!(tokens[1], TokenKind::Identifier("RegisterUser".into()));
        assert_eq!(tokens[2], TokenKind::LBrace);
        assert_eq!(tokens[3], TokenKind::Describe);
        assert_eq!(
            tokens[4],
            TokenKind::StringLiteral("A new user registers".into())
        );
        assert_eq!(tokens[5], TokenKind::Given);
        assert_eq!(
            tokens[6],
            TokenKind::FreeText("a user with valid credentials".into())
        );
        assert_eq!(tokens[7], TokenKind::When);
        assert_eq!(
            tokens[8],
            TokenKind::FreeText("the user submits the registration form".into())
        );
        assert_eq!(tokens[9], TokenKind::Then);
        assert_eq!(
            tokens[10],
            TokenKind::FreeText("the system creates a new account".into())
        );
        assert_eq!(tokens[11], TokenKind::RBrace);
    }

    #[test]
    fn tokenizes_complete_edge_definition() {
        let input = r#"RegisterUser -> LoginUser {
    passes { user_id, email }
    describe "After registration, the user can log in"
}"#;
        let tokens = kinds_no_newlines(input);
        assert_eq!(tokens[0], TokenKind::Identifier("RegisterUser".into()));
        assert_eq!(tokens[1], TokenKind::Arrow);
        assert_eq!(tokens[2], TokenKind::Identifier("LoginUser".into()));
        assert_eq!(tokens[3], TokenKind::LBrace);
        assert_eq!(tokens[4], TokenKind::Passes);
        assert_eq!(tokens[5], TokenKind::LBrace);
        assert_eq!(tokens[6], TokenKind::Identifier("user_id".into()));
        assert_eq!(tokens[7], TokenKind::Comma);
        assert_eq!(tokens[8], TokenKind::Identifier("email".into()));
        assert_eq!(tokens[9], TokenKind::RBrace);
        assert_eq!(tokens[10], TokenKind::Describe);
        assert_eq!(
            tokens[11],
            TokenKind::StringLiteral("After registration, the user can log in".into())
        );
        assert_eq!(tokens[12], TokenKind::RBrace);
    }

    #[test]
    fn tokenizes_multiline_node() {
        let input = "node A {\n  given something\n  then something else\n}";
        let tokens = tokenize(input).expect("should succeed");
        // Ensure newlines are present between lines
        let newline_count = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Newline)
            .count();
        assert_eq!(newline_count, 3);
    }

    #[test]
    fn rejects_unterminated_string() {
        let result = tokenize(r#""hello world"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unterminated string"));
        assert_eq!(err.span.line, 1);
        assert_eq!(err.span.col, 1);
    }

    #[test]
    fn tracks_line_and_column_spans() {
        let input = "graph MyGraph {\n  node Foo {\n  }\n}";
        let tokens = tokenize(input).expect("should succeed");

        // "graph" at line 1, col 1
        assert_eq!(tokens[0].span.line, 1);
        assert_eq!(tokens[0].span.col, 1);

        // "MyGraph" at line 1, col 7
        assert_eq!(tokens[1].span.line, 1);
        assert_eq!(tokens[1].span.col, 7);

        // "{" at line 1, col 15
        assert_eq!(tokens[2].span.line, 1);
        assert_eq!(tokens[2].span.col, 15);

        // newline at line 1
        assert_eq!(tokens[3].kind, TokenKind::Newline);
        assert_eq!(tokens[3].span.line, 1);

        // "node" at line 2, col 3
        assert_eq!(tokens[4].span.line, 2);
        assert_eq!(tokens[4].span.col, 3);

        // "Foo" at line 2, col 8
        assert_eq!(tokens[5].span.line, 2);
        assert_eq!(tokens[5].span.col, 8);
    }

    #[test]
    fn tokenizes_free_text_stops_at_data_block() {
        let tokens = kinds("given a user with {\n  email: \"x\"\n}");
        // given -> FreeText("a user with") -> LBrace -> ...
        assert_eq!(tokens[0], TokenKind::Given);
        assert_eq!(tokens[1], TokenKind::FreeText("a user with".into()));
        assert_eq!(tokens[2], TokenKind::LBrace);
    }

    #[test]
    fn tokenizes_and_but_with_free_text() {
        let tokens = kinds("and the email is sent");
        assert_eq!(
            tokens,
            vec![
                TokenKind::And,
                TokenKind::FreeText("the email is sent".into()),
            ]
        );

        let tokens = kinds("but no duplicate records exist");
        assert_eq!(
            tokens,
            vec![
                TokenKind::But,
                TokenKind::FreeText("no duplicate records exist".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_describe_with_string() {
        let tokens = kinds_no_newlines(r#"describe "A test scenario""#);
        assert_eq!(
            tokens,
            vec![
                TokenKind::Describe,
                TokenKind::StringLiteral("A test scenario".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_parameter_in_free_text() {
        let tokens = kinds("given a user with email <email>");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Given,
                TokenKind::FreeText("a user with email".into()),
                TokenKind::Parameter("email".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_multiple_parameters_in_free_text() {
        let tokens = kinds("when the user enters <username> and <password>");
        assert_eq!(
            tokens,
            vec![
                TokenKind::When,
                TokenKind::FreeText("the user enters".into()),
                TokenKind::Parameter("username".into()),
                TokenKind::FreeText("and".into()),
                TokenKind::Parameter("password".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_parameter_preserves_surrounding_text() {
        let tokens = kinds("then the response status is <status_code> and body contains <message>");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Then,
                TokenKind::FreeText("the response status is".into()),
                TokenKind::Parameter("status_code".into()),
                TokenKind::FreeText("and body contains".into()),
                TokenKind::Parameter("message".into()),
            ]
        );
    }

    #[test]
    fn tokenizes_free_text_without_parameters_unchanged() {
        let tokens = kinds("given a user with valid credentials");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Given,
                TokenKind::FreeText("a user with valid credentials".into()),
            ]
        );
    }
}
