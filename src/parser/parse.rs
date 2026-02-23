use crate::parser::ast::{
    DataBlock, Edge, Fixture, Graph, Import, Node, Step, StepType, Tag, Value,
};
use crate::parser::error::ParseError;
use crate::parser::lexer::{Token, TokenKind, tokenize};
use crate::util::span::Span;

/// Parse a `.tast` source string into a list of top-level graphs and imports.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input contains invalid syntax.
pub fn parse(input: &str) -> Result<Vec<Graph>, ParseError> {
    let tokens = tokenize(input).map_err(|e| ParseError {
        message: e.message,
        span: e.span,
    })?;
    let mut parser = Parser::new(&tokens);
    parser.parse_file()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Returns the current token, or `None` if at end.
    fn peek(&self) -> Option<&Token> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if self.tokens[i].kind == TokenKind::Newline
                || matches!(self.tokens[i].kind, TokenKind::Comment(_))
            {
                i += 1;
            } else {
                return Some(&self.tokens[i]);
            }
        }
        None
    }

    /// Returns the kind of the current non-whitespace token.
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    /// Advance past any newlines and comments, then return and consume the next token.
    fn next_token(&mut self) -> Option<&Token> {
        self.skip_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    /// Skip newlines and comments.
    fn skip_trivia(&mut self) {
        while self.pos < self.tokens.len() {
            match &self.tokens[self.pos].kind {
                TokenKind::Newline | TokenKind::Comment(_) => self.pos += 1,
                _ => break,
            }
        }
    }

    /// Expect a specific token kind, or return an error.
    fn expect(&mut self, expected: &TokenKind) -> Result<Span, ParseError> {
        let tok = self.next_token();
        match tok {
            Some(t) if t.kind == *expected => Ok(t.span),
            Some(t) => Err(ParseError {
                message: format!("expected {}, found {:?}", token_name(expected), t.kind),
                span: t.span,
            }),
            None => Err(ParseError {
                message: format!("expected {}, found end of input", token_name(expected)),
                span: self.eof_span(),
            }),
        }
    }

    /// Expect an identifier and return its name.
    fn expect_identifier(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.next_token();
        match tok {
            Some(Token {
                kind: TokenKind::Identifier(name),
                span,
            }) => Ok((name.clone(), *span)),
            Some(t) => Err(ParseError {
                message: format!("expected identifier, found {:?}", t.kind),
                span: t.span,
            }),
            None => Err(ParseError {
                message: "expected identifier, found end of input".to_owned(),
                span: self.eof_span(),
            }),
        }
    }

    /// Returns a span for the end of input.
    fn eof_span(&self) -> Span {
        if let Some(last) = self.tokens.last() {
            Span::new(last.span.end, last.span.end, last.span.line, last.span.col)
        } else {
            Span::default()
        }
    }

    /// Parse an entire file: a sequence of imports and graphs.
    fn parse_file(&mut self) -> Result<Vec<Graph>, ParseError> {
        let mut graphs = Vec::new();
        loop {
            self.skip_trivia();
            if self.pos >= self.tokens.len() {
                break;
            }
            match self.peek_kind() {
                Some(TokenKind::Graph) => {
                    graphs.push(self.parse_graph()?);
                }
                Some(TokenKind::Import) => {
                    // Imports are attached to the next graph, but for now
                    // we'll parse them standalone. Phase 2 handles multi-file.
                    let import = self.parse_import()?;
                    // If there's a graph following, attach the import to it.
                    // Otherwise, create a placeholder. For now, just store and continue.
                    if let Some(graph) = graphs.last_mut() {
                        graph.imports.push(import);
                    } else {
                        // Import before any graph — we'll attach to the next graph
                        // by collecting imports first, then adding to the graph.
                        let mut remaining = self.parse_file()?;
                        if let Some(first) = remaining.first_mut() {
                            first.imports.insert(0, import);
                        }
                        return Ok(remaining);
                    }
                }
                Some(other) => {
                    let tok = self.peek().unwrap();
                    return Err(ParseError {
                        message: format!("expected 'graph' or 'import', found {:?}", other),
                        span: tok.span,
                    });
                }
                None => break,
            }
        }
        Ok(graphs)
    }

    /// Parse: `graph Name { ... }`
    fn parse_graph(&mut self) -> Result<Graph, ParseError> {
        let start_span = self.expect(&TokenKind::Graph)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::LBrace)?;

        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut fixtures: Vec<Fixture> = Vec::new();
        let mut config = None;

        loop {
            self.skip_trivia();
            match self.peek_kind() {
                Some(TokenKind::RBrace) => {
                    let end_span = self.expect(&TokenKind::RBrace)?;
                    let span = start_span.merge(end_span);

                    // Validate no duplicate node names
                    let mut seen = std::collections::HashSet::new();
                    for node in &nodes {
                        if !seen.insert(&node.name) {
                            return Err(ParseError {
                                message: format!("duplicate node name '{}'", node.name),
                                span: node.span,
                            });
                        }
                    }

                    // Validate edge references (skip dotted names — those are cross-graph refs)
                    let node_names: std::collections::HashSet<&str> =
                        nodes.iter().map(|n| n.name.as_str()).collect();
                    for edge in &edges {
                        if !edge.from.contains('.') && !node_names.contains(edge.from.as_str()) {
                            return Err(ParseError {
                                message: format!("edge references unknown node '{}'", edge.from),
                                span: edge.span,
                            });
                        }
                        if !edge.to.contains('.') && !node_names.contains(edge.to.as_str()) {
                            return Err(ParseError {
                                message: format!("edge references unknown node '{}'", edge.to),
                                span: edge.span,
                            });
                        }
                    }

                    return Ok(Graph {
                        name,
                        nodes,
                        edges,
                        config,
                        imports: vec![],
                        fixtures,
                        span,
                    });
                }
                Some(TokenKind::Node) => {
                    nodes.push(self.parse_node()?);
                }
                Some(TokenKind::Fixture) => {
                    fixtures.push(self.parse_fixture()?);
                }
                Some(TokenKind::Config) => {
                    config = Some(self.parse_config_block()?);
                }
                Some(TokenKind::Identifier(_)) => {
                    // Could be an edge: Identifier -> Identifier { ... }
                    edges.push(self.parse_edge()?);
                }
                None => {
                    return Err(ParseError {
                        message: "unclosed graph, expected '}'".to_owned(),
                        span: self.eof_span(),
                    });
                }
                Some(other) => {
                    let tok = self.peek().unwrap();
                    return Err(ParseError {
                        message: format!(
                            "unexpected {:?} inside graph, expected 'node', edge, or '}}'",
                            other
                        ),
                        span: tok.span,
                    });
                }
            }
        }
    }

    /// Parse: `node Name { ... }`
    fn parse_node(&mut self) -> Result<Node, ParseError> {
        let start_span = self.expect(&TokenKind::Node)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::LBrace)?;

        let mut description = None;
        let mut steps = Vec::new();
        let mut tags = Vec::new();
        let mut requires = Vec::new();
        let mut config = None;

        loop {
            self.skip_trivia();
            match self.peek_kind() {
                Some(TokenKind::RBrace) => {
                    let end_span = self.expect(&TokenKind::RBrace)?;
                    return Ok(Node {
                        name,
                        description,
                        steps,
                        tags,
                        requires,
                        config,
                        span: start_span.merge(end_span),
                    });
                }
                Some(TokenKind::Describe) => {
                    self.next_token(); // consume 'describe'
                    let tok = self.next_token();
                    match tok {
                        Some(Token {
                            kind: TokenKind::StringLiteral(s),
                            ..
                        }) => {
                            description = Some(s.clone());
                        }
                        Some(t) => {
                            return Err(ParseError {
                                message: format!(
                                    "expected string after 'describe', found {:?}",
                                    t.kind
                                ),
                                span: t.span,
                            });
                        }
                        None => {
                            return Err(ParseError {
                                message: "expected string after 'describe', found end of input"
                                    .to_owned(),
                                span: self.eof_span(),
                            });
                        }
                    }
                }
                Some(TokenKind::Given)
                | Some(TokenKind::When)
                | Some(TokenKind::Then)
                | Some(TokenKind::And)
                | Some(TokenKind::But) => {
                    steps.push(self.parse_step()?);
                }
                Some(TokenKind::Tags) => {
                    tags = self.parse_tags()?;
                }
                Some(TokenKind::Requires) => {
                    requires = self.parse_requires()?;
                }
                Some(TokenKind::Config) => {
                    config = Some(self.parse_config_block()?);
                }
                None => {
                    return Err(ParseError {
                        message: "unclosed node, expected '}'".to_owned(),
                        span: self.eof_span(),
                    });
                }
                Some(other) => {
                    let tok = self.peek().unwrap();
                    return Err(ParseError {
                        message: format!("unexpected {:?} inside node", other),
                        span: tok.span,
                    });
                }
            }
        }
    }

    /// Parse a step: `given/when/then/and/but <free text> [{ data }]`
    fn parse_step(&mut self) -> Result<Step, ParseError> {
        let tok = self.next_token().unwrap();
        let step_type = match &tok.kind {
            TokenKind::Given => StepType::Given,
            TokenKind::When => StepType::When,
            TokenKind::Then => StepType::Then,
            TokenKind::And => StepType::And,
            TokenKind::But => StepType::But,
            _ => unreachable!("parse_step called on non-step token"),
        };
        let start_span = tok.span;

        // The lexer already captured free text after the keyword.
        // Check if the next token is FreeText.
        let mut text = String::new();
        self.skip_trivia();
        if self.pos < self.tokens.len()
            && let TokenKind::FreeText(ref t) = self.tokens[self.pos].kind
        {
            text = t.clone();
            self.pos += 1;
        }

        // Check for inline data block
        let data = if self.peek_kind() == Some(&TokenKind::LBrace) {
            Some(self.parse_data_block()?)
        } else {
            None
        };

        Ok(Step {
            step_type,
            text,
            data,
            span: start_span,
        })
    }

    /// Parse: `tags [tag1, tag2, ...]`
    fn parse_tags(&mut self) -> Result<Vec<Tag>, ParseError> {
        self.next_token(); // consume 'tags'
        self.expect(&TokenKind::LBracket)?;
        let mut tags = Vec::new();
        loop {
            match self.peek_kind() {
                Some(TokenKind::RBracket) => {
                    self.next_token();
                    return Ok(tags);
                }
                Some(TokenKind::Identifier(_)) => {
                    let (name, _) = self.expect_identifier()?;
                    tags.push(Tag(name));
                    // Optional comma
                    if self.peek_kind() == Some(&TokenKind::Comma) {
                        self.next_token();
                    }
                }
                _ => {
                    let span = self.peek().map_or(self.eof_span(), |t| t.span);
                    return Err(ParseError {
                        message: "expected tag name or ']'".to_owned(),
                        span,
                    });
                }
            }
        }
    }

    /// Parse: `requires { field1, field2, ... }`
    fn parse_requires(&mut self) -> Result<Vec<String>, ParseError> {
        self.next_token(); // consume 'requires'
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        loop {
            match self.peek_kind() {
                Some(TokenKind::RBrace) => {
                    self.next_token();
                    return Ok(fields);
                }
                Some(TokenKind::Identifier(_)) => {
                    let (name, _) = self.expect_identifier()?;
                    fields.push(name);
                    if self.peek_kind() == Some(&TokenKind::Comma) {
                        self.next_token();
                    }
                }
                _ => {
                    let span = self.peek().map_or(self.eof_span(), |t| t.span);
                    return Err(ParseError {
                        message: "expected field name or '}'".to_owned(),
                        span,
                    });
                }
            }
        }
    }

    /// Parse an edge: `FromNode -> ToNode { ... }`
    fn parse_edge(&mut self) -> Result<Edge, ParseError> {
        let (from, start_span) = self.expect_identifier()?;

        // Handle dotted identifiers (e.g., Auth.Login)
        let from = if self.peek_kind() == Some(&TokenKind::Dot) {
            self.next_token(); // consume '.'
            let (rest, _) = self.expect_identifier()?;
            format!("{from}.{rest}")
        } else {
            from
        };

        self.expect(&TokenKind::Arrow)?;
        let (to, _) = self.expect_identifier()?;

        // Handle dotted identifiers for target
        let to = if self.peek_kind() == Some(&TokenKind::Dot) {
            self.next_token(); // consume '.'
            let (rest, _) = self.expect_identifier()?;
            format!("{to}.{rest}")
        } else {
            to
        };

        // Edge body is optional
        let (passes, description, end_span) = if self.peek_kind() == Some(&TokenKind::LBrace) {
            self.expect(&TokenKind::LBrace)?;
            let mut passes = Vec::new();
            let mut desc = None;

            loop {
                self.skip_trivia();
                match self.peek_kind() {
                    Some(TokenKind::RBrace) => {
                        let end = self.expect(&TokenKind::RBrace)?;
                        break (passes, desc, end);
                    }
                    Some(TokenKind::Passes) => {
                        self.next_token(); // consume 'passes'
                        passes = self.parse_identifier_list()?;
                    }
                    Some(TokenKind::Describe) => {
                        self.next_token(); // consume 'describe'
                        let tok = self.next_token();
                        match tok {
                            Some(Token {
                                kind: TokenKind::StringLiteral(s),
                                ..
                            }) => desc = Some(s.clone()),
                            Some(t) => {
                                return Err(ParseError {
                                    message: format!(
                                        "expected string after 'describe', found {:?}",
                                        t.kind
                                    ),
                                    span: t.span,
                                });
                            }
                            None => {
                                return Err(ParseError {
                                    message: "expected string after 'describe'".to_owned(),
                                    span: self.eof_span(),
                                });
                            }
                        }
                    }
                    None => {
                        return Err(ParseError {
                            message: "unclosed edge block, expected '}'".to_owned(),
                            span: self.eof_span(),
                        });
                    }
                    Some(other) => {
                        let tok = self.peek().unwrap();
                        return Err(ParseError {
                            message: format!("unexpected {:?} inside edge block", other),
                            span: tok.span,
                        });
                    }
                }
            }
        } else {
            (vec![], None, start_span)
        };

        Ok(Edge {
            from,
            to,
            passes,
            description,
            span: start_span.merge(end_span),
        })
    }

    /// Parse: `{ ident, ident, ... }`
    fn parse_identifier_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut names = Vec::new();
        loop {
            match self.peek_kind() {
                Some(TokenKind::RBrace) => {
                    self.next_token();
                    return Ok(names);
                }
                Some(TokenKind::Identifier(_)) => {
                    let (name, _) = self.expect_identifier()?;
                    names.push(name);
                    if self.peek_kind() == Some(&TokenKind::Comma) {
                        self.next_token();
                    }
                }
                _ => {
                    let span = self.peek().map_or(self.eof_span(), |t| t.span);
                    return Err(ParseError {
                        message: "expected identifier or '}'".to_owned(),
                        span,
                    });
                }
            }
        }
    }

    /// Parse: `{ key: value, ... }` as a DataBlock.
    fn parse_data_block(&mut self) -> Result<DataBlock, ParseError> {
        let start = self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();

        loop {
            match self.peek_kind() {
                Some(TokenKind::RBrace) => {
                    let end = self.expect(&TokenKind::RBrace)?;
                    return Ok(DataBlock {
                        fields,
                        span: start.merge(end),
                    });
                }
                Some(TokenKind::Identifier(_)) => {
                    let (key, _) = self.expect_identifier()?;
                    self.expect(&TokenKind::Colon)?;
                    let value = self.parse_value()?;
                    fields.push((key, value));
                    if self.peek_kind() == Some(&TokenKind::Comma) {
                        self.next_token();
                    }
                }
                _ => {
                    let span = self.peek().map_or(self.eof_span(), |t| t.span);
                    return Err(ParseError {
                        message: "expected field name or '}' in data block".to_owned(),
                        span,
                    });
                }
            }
        }
    }

    /// Parse a value: string literal, identifier (as string), or boolean/null.
    fn parse_value(&mut self) -> Result<Value, ParseError> {
        let tok = self.next_token();
        match tok {
            Some(Token {
                kind: TokenKind::StringLiteral(s),
                ..
            }) => Ok(Value::String(s.clone())),
            Some(Token {
                kind: TokenKind::Identifier(s),
                ..
            }) => match s.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "null" => Ok(Value::Null),
                _ => Ok(Value::String(s.clone())),
            },
            Some(t) => Err(ParseError {
                message: format!("expected value, found {:?}", t.kind),
                span: t.span,
            }),
            None => Err(ParseError {
                message: "expected value, found end of input".to_owned(),
                span: self.eof_span(),
            }),
        }
    }

    /// Parse: `config { key: value, ... }`
    fn parse_config_block(&mut self) -> Result<DataBlock, ParseError> {
        self.next_token(); // consume 'config'
        self.parse_data_block()
    }

    /// Parse: `import Name from "path"`
    fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start_span = self.expect(&TokenKind::Import)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::From)?;
        let tok = self.next_token();
        match tok {
            Some(Token {
                kind: TokenKind::StringLiteral(path),
                span: end_span,
            }) => Ok(Import {
                name,
                path: path.clone(),
                span: start_span.merge(*end_span),
            }),
            Some(t) => Err(ParseError {
                message: format!("expected path string after 'from', found {:?}", t.kind),
                span: t.span,
            }),
            None => Err(ParseError {
                message: "expected path string after 'from'".to_owned(),
                span: self.eof_span(),
            }),
        }
    }

    /// Parse: `fixture Name { key: value, ... }`
    fn parse_fixture(&mut self) -> Result<Fixture, ParseError> {
        let start_span = self.expect(&TokenKind::Fixture)?;
        let (name, _) = self.expect_identifier()?;
        let fields = self.parse_data_block()?;
        Ok(Fixture {
            name,
            fields,
            span: start_span,
        })
    }
}

/// Human-readable name for a token kind (for error messages).
fn token_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Graph => "'graph'",
        TokenKind::Node => "'node'",
        TokenKind::Describe => "'describe'",
        TokenKind::Given => "'given'",
        TokenKind::When => "'when'",
        TokenKind::Then => "'then'",
        TokenKind::And => "'and'",
        TokenKind::But => "'but'",
        TokenKind::Passes => "'passes'",
        TokenKind::Requires => "'requires'",
        TokenKind::Tags => "'tags'",
        TokenKind::Config => "'config'",
        TokenKind::Import => "'import'",
        TokenKind::Fixture => "'fixture'",
        TokenKind::From => "'from'",
        TokenKind::LBrace => "'{'",
        TokenKind::RBrace => "'}'",
        TokenKind::LBracket => "'['",
        TokenKind::RBracket => "']'",
        TokenKind::Arrow => "'->'",
        TokenKind::Colon => "':'",
        TokenKind::Comma => "','",
        TokenKind::Dot => "'.'",
        TokenKind::Newline => "newline",
        TokenKind::StringLiteral(_) => "string literal",
        TokenKind::Identifier(_) => "identifier",
        TokenKind::FreeText(_) => "text",
        TokenKind::Comment(_) => "comment",
        #[allow(unreachable_patterns)]
        _ => "token",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a string and return the first graph.
    fn parse_one(input: &str) -> Graph {
        let graphs = parse(input).expect("parse failed");
        assert_eq!(graphs.len(), 1);
        graphs.into_iter().next().unwrap()
    }

    // ── Valid input tests ──────────────────────────────────────

    #[test]
    fn parses_empty_graph() {
        let graph = parse_one("graph Empty {}");
        assert_eq!(graph.name, "Empty");
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn parses_graph_with_name() {
        let graph = parse_one("graph UserAuthentication {}");
        assert_eq!(graph.name, "UserAuthentication");
    }

    #[test]
    fn parses_graph_with_single_empty_node() {
        let graph = parse_one("graph G { node A {} }");
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].name, "A");
    }

    #[test]
    fn parses_graph_with_node_and_description() {
        let graph = parse_one(
            r#"graph G {
                node Register {
                    describe "A new user registers"
                }
            }"#,
        );
        assert_eq!(
            graph.nodes[0].description.as_deref(),
            Some("A new user registers")
        );
    }

    #[test]
    fn parses_node_with_given_step() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    given a registered user
                }
            }"#,
        );
        assert_eq!(graph.nodes[0].steps.len(), 1);
        assert_eq!(graph.nodes[0].steps[0].step_type, StepType::Given);
        assert_eq!(graph.nodes[0].steps[0].text, "a registered user");
    }

    #[test]
    fn parses_node_with_when_step() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    when the user submits the form
                }
            }"#,
        );
        assert_eq!(graph.nodes[0].steps[0].step_type, StepType::When);
        assert_eq!(graph.nodes[0].steps[0].text, "the user submits the form");
    }

    #[test]
    fn parses_node_with_then_step() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    then the system creates an account
                }
            }"#,
        );
        assert_eq!(graph.nodes[0].steps[0].step_type, StepType::Then);
    }

    #[test]
    fn parses_node_with_and_continuation() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    then the account is created
                    and the user receives an email
                }
            }"#,
        );
        assert_eq!(graph.nodes[0].steps.len(), 2);
        assert_eq!(graph.nodes[0].steps[1].step_type, StepType::And);
        assert_eq!(graph.nodes[0].steps[1].text, "the user receives an email");
    }

    #[test]
    fn parses_node_with_but_continuation() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    then a confirmation page is shown
                    but no duplicate records exist
                }
            }"#,
        );
        assert_eq!(graph.nodes[0].steps[1].step_type, StepType::But);
    }

    #[test]
    fn parses_node_with_full_given_when_then() {
        let graph = parse_one(
            r#"graph G {
                node Login {
                    describe "User logs in"
                    given a registered user
                    when the user submits valid credentials
                    then the system returns an auth token
                    and the session is active
                }
            }"#,
        );
        let node = &graph.nodes[0];
        assert_eq!(node.description.as_deref(), Some("User logs in"));
        assert_eq!(node.steps.len(), 4);
        assert_eq!(node.steps[0].step_type, StepType::Given);
        assert_eq!(node.steps[1].step_type, StepType::When);
        assert_eq!(node.steps[2].step_type, StepType::Then);
        assert_eq!(node.steps[3].step_type, StepType::And);
    }

    #[test]
    fn parses_node_with_inline_data_block() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    given a user with {
                        email: "test@example.com"
                        password: "secure123"
                    }
                }
            }"#,
        );
        let step = &graph.nodes[0].steps[0];
        assert_eq!(step.text, "a user with");
        let data = step.data.as_ref().expect("should have data block");
        assert_eq!(data.fields.len(), 2);
        assert_eq!(data.fields[0].0, "email");
        assert_eq!(data.fields[0].1, Value::String("test@example.com".into()));
    }

    #[test]
    fn parses_node_with_string_data_in_step() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    given a user with {
                        role: "admin"
                    }
                }
            }"#,
        );
        let data = graph.nodes[0].steps[0].data.as_ref().unwrap();
        assert_eq!(data.fields[0].1, Value::String("admin".into()));
    }

    #[test]
    fn parses_node_with_tags() {
        let graph = parse_one(
            r#"graph G {
                node A {
                    tags [smoke, critical]
                }
            }"#,
        );
        assert_eq!(
            graph.nodes[0].tags,
            vec![Tag("smoke".into()), Tag("critical".into())]
        );
    }

    #[test]
    fn parses_node_with_requires() {
        let graph = parse_one(
            r#"graph G {
                node Dashboard {
                    requires { auth_token }
                }
            }"#,
        );
        assert_eq!(graph.nodes[0].requires, vec!["auth_token"]);
    }

    #[test]
    fn parses_edge_simple() {
        let graph = parse_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].from, "A");
        assert_eq!(graph.edges[0].to, "B");
    }

    #[test]
    fn parses_edge_with_passes() {
        let graph = parse_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B {
                    passes { user_id, email }
                }
            }"#,
        );
        assert_eq!(graph.edges[0].passes, vec!["user_id", "email"]);
    }

    #[test]
    fn parses_edge_with_description() {
        let graph = parse_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B {
                    describe "A leads to B"
                }
            }"#,
        );
        assert_eq!(graph.edges[0].description.as_deref(), Some("A leads to B"));
    }

    #[test]
    fn parses_edge_with_passes_and_description() {
        let graph = parse_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B {
                    passes { token }
                    describe "Auth flow"
                }
            }"#,
        );
        assert_eq!(graph.edges[0].passes, vec!["token"]);
        assert_eq!(graph.edges[0].description.as_deref(), Some("Auth flow"));
    }

    #[test]
    fn parses_graph_with_multiple_nodes() {
        let graph = parse_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
            }"#,
        );
        assert_eq!(graph.nodes.len(), 3);
    }

    #[test]
    fn parses_graph_with_multiple_edges() {
        let graph = parse_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn parses_full_example_auth_graph() {
        let input = r#"graph UserAuthentication {
            node RegisterUser {
                describe "A new user registers with valid credentials"
                given a user with {
                    email: "test@example.com"
                    password: "secure123"
                }
                when the user submits the registration form
                then the system creates a new account
                and the user receives a confirmation email
            }

            node LoginUser {
                describe "A registered user logs in"
                given a registered user with email "test@example.com"
                when the user submits valid credentials
                then the system returns an auth token
                and the session is active
            }

            node AccessDashboard {
                describe "An authenticated user accesses the dashboard"
                given an active session
                when the user navigates to /dashboard
                then the dashboard loads with user-specific data
            }

            node LogoutUser {
                describe "A user logs out of their session"
                given an active session
                when the user clicks logout
                then the session is destroyed
                and the user is redirected to the login page
            }

            RegisterUser -> LoginUser {
                passes { user_id, email }
                describe "After registration, the user can log in"
            }

            LoginUser -> AccessDashboard {
                passes { auth_token, session_id }
                describe "Login grants access to protected routes"
            }

            LoginUser -> LogoutUser {
                passes { session_id }
                describe "A logged-in user can log out"
            }
        }"#;
        let graph = parse_one(input);
        assert_eq!(graph.name, "UserAuthentication");
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.edges[0].from, "RegisterUser");
        assert_eq!(graph.edges[0].to, "LoginUser");
        assert_eq!(graph.edges[0].passes, vec!["user_id", "email"]);
    }

    #[test]
    fn parses_graph_with_config() {
        let graph = parse_one(
            r#"graph G {
                config { timeout: "30s" }
                node A {}
            }"#,
        );
        let config = graph.config.as_ref().expect("should have config");
        assert_eq!(config.fields[0].0, "timeout");
        assert_eq!(config.fields[0].1, Value::String("30s".into()));
    }

    #[test]
    fn parses_import_statement() {
        let input = r#"import Auth from "./auth.tast"
            graph G {
                node A {}
            }"#;
        let graphs = parse(input).expect("parse failed");
        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].imports.len(), 1);
        assert_eq!(graphs[0].imports[0].name, "Auth");
        assert_eq!(graphs[0].imports[0].path, "./auth.tast");
    }

    #[test]
    fn parses_fixture_definition() {
        let graph = parse_one(
            r#"graph G {
                fixture AdminUser { role: "admin" }
                node A {}
            }"#,
        );
        assert_eq!(graph.fixtures.len(), 1);
        assert_eq!(graph.fixtures[0].name, "AdminUser");
    }

    // ── Error case tests ───────────────────────────────────────

    #[test]
    fn error_missing_graph_name() {
        let result = parse("graph {}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("expected identifier"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn error_missing_graph_brace() {
        let result = parse("graph G");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("'{'"), "got: {}", err.message);
    }

    #[test]
    fn error_missing_node_name() {
        let result = parse("graph G { node {} }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("expected identifier"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn error_missing_node_brace() {
        let result = parse("graph G { node A }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("'{'"), "got: {}", err.message);
    }

    #[test]
    fn error_unclosed_graph() {
        let result = parse("graph G { node A {}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("unclosed graph"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn error_unclosed_node() {
        let result = parse("graph G { node A { given something");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("unclosed node"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn error_edge_missing_target() {
        let result = parse("graph G { node A {} A -> }");
        assert!(result.is_err());
    }

    #[test]
    fn error_edge_invalid_arrow() {
        // "-" without ">" — the lexer skips stray '-', so "A - B" won't produce Arrow.
        // Instead "A" is ident, "-" is skipped, "B" is another ident, which triggers
        // the edge parser on "A" but there's no Arrow next.
        let result = parse("graph G { node A {} node B {} A - B }");
        assert!(result.is_err());
    }

    #[test]
    fn error_unterminated_string_in_step() {
        let result = parse(r#"graph G { node A { describe "unclosed } }"#);
        assert!(result.is_err());
    }

    #[test]
    fn error_invalid_data_block_syntax() {
        let result = parse(r#"graph G { node A { given x { : } } }"#);
        assert!(result.is_err());
    }

    #[test]
    fn error_duplicate_node_names() {
        let result = parse("graph G { node A {} node A {} }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("duplicate node name"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn error_step_outside_node() {
        let result = parse("graph G { given something }");
        assert!(result.is_err());
    }

    #[test]
    fn error_edge_referencing_unknown_node() {
        let result = parse("graph G { node A {} A -> Unknown }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unknown node"), "got: {}", err.message);
    }

    #[test]
    fn error_reports_line_number() {
        let result = parse("graph G {\n  node A {\n    describe 42\n  }\n}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.span.line >= 3,
            "expected line >= 3, got {}",
            err.span.line
        );
    }

    #[test]
    fn error_reports_column_number() {
        let result = parse("graph {}");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.span.col > 0, "column should be > 0");
    }

    #[test]
    fn error_reports_helpful_message() {
        let result = parse("graph G { node A { describe 42 } }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should mention what was expected
        assert!(
            err.message.contains("expected") || err.message.contains("string"),
            "message should be helpful, got: {}",
            err.message
        );
    }
}
