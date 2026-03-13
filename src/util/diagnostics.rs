use std::io::Write;

use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::parser::error::ParseError;

/// Render a parse error as a rich diagnostic to a writer.
///
/// Produces source-annotated output with span underlines, filenames,
/// and line numbers using ariadne.
///
/// # Errors
///
/// Returns an I/O error if writing to `w` fails.
pub fn render_parse_error<W: Write>(
    filename: &str,
    source: &str,
    error: &ParseError,
    w: W,
) -> std::io::Result<()> {
    let range = error.span.start..error.span.end;

    let mut report = Report::build(ReportKind::Error, filename, error.span.start)
        .with_message(&error.message)
        .with_label(
            Label::new((filename, range))
                .with_message(&error.message)
                .with_color(Color::Red),
        );

    for secondary in &error.secondary {
        let sec_range = secondary.span.start..secondary.span.end;
        report = report.with_label(
            Label::new((filename, sec_range))
                .with_message(&secondary.message)
                .with_color(Color::Blue),
        );
    }

    if let Some(help) = &error.help {
        report = report.with_help(help);
    }

    report
        .finish()
        .write_for_stdout((filename, Source::from(source)), w)
}

/// Render a parse error as a rich diagnostic to stderr.
pub fn eprint_parse_error(filename: &str, source: &str, error: &ParseError) {
    let stderr = std::io::stderr();
    let _ = render_parse_error(filename, source, error, stderr.lock());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::span::Span;

    fn render_to_string(filename: &str, source: &str, error: &ParseError) -> String {
        let mut buf = Vec::new();
        render_parse_error(filename, source, error, &mut buf).expect("write failed");
        let raw = String::from_utf8(buf).expect("invalid utf8");
        // Strip ANSI escape codes for test assertions.
        strip_ansi(&raw)
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm' (end of ANSI escape sequence).
                for c2 in chars.by_ref() {
                    if c2 == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn diagnostic_renders_parse_error() {
        let source = "graph G {}\nnode X";
        let error = ParseError {
            message: "expected '{' after node name".to_string(),
            span: Span::new(16, 17, 2, 6),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("expected '{' after node name"),
            "output should contain error message, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_includes_filename() {
        let source = "graph G {}";
        let error = ParseError {
            message: "unexpected token".to_string(),
            span: Span::new(0, 5, 1, 1),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("my_file.tast", source, &error);
        assert!(
            output.contains("my_file.tast"),
            "output should contain filename, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_includes_source_line() {
        let source = "graph BadSyntax here";
        let error = ParseError {
            message: "expected '{'".to_string(),
            span: Span::new(16, 20, 1, 17),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("graph BadSyntax here"),
            "output should contain the source line, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_handles_eof_span() {
        let source = "graph G {";
        let len = source.len();
        let error = ParseError {
            message: "unexpected end of file".to_string(),
            span: Span::new(len, len, 1, len + 1),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("unexpected end of file"),
            "output should contain eof error message, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_underlines_span_range() {
        // The span covers "BadToken" (bytes 6..14 on line 1).
        let source = "graph BadToken {}";
        let error = ParseError {
            message: "unexpected identifier".to_string(),
            span: Span::new(6, 14, 1, 7),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        // Ariadne renders underline characters (─ or ╰) under the span.
        // The key indicator is the box-drawing line pointing at the span.
        assert!(
            output.contains('─'),
            "output should contain underline characters, got:\n{output}"
        );
        assert!(
            output.contains("unexpected identifier"),
            "output should contain label under the span, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_eof_error_points_to_end() {
        // EOF span at byte offset == source length, zero-width.
        let source = "graph G {\n  node A {}\n";
        let len = source.len();
        let error = ParseError {
            message: "expected '}'".to_string(),
            span: Span::new(len, len, 3, 1),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("expected '}'"),
            "output should render eof error, got:\n{output}"
        );
        // Should still show the filename location.
        assert!(
            output.contains("test.tast"),
            "output should contain filename for eof error, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_zero_length_span() {
        // Zero-length span points between characters (insertion point).
        let source = "graph G node A {}";
        let error = ParseError {
            message: "expected '{'".to_string(),
            span: Span::new(8, 8, 1, 9),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("expected '{'"),
            "output should render zero-length span, got:\n{output}"
        );
        assert!(
            output.contains("graph G node A {}"),
            "output should show the source line, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_multichar_span() {
        // Span covering an entire keyword "describe" (8 bytes).
        let source = "graph G {\n  describe\n}";
        let error = ParseError {
            message: "expected 'node', found 'describe'".to_string(),
            span: Span::new(12, 20, 2, 3),
            secondary: vec![],
            help: None,
        };
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("expected 'node', found 'describe'"),
            "output should contain the multi-char error, got:\n{output}"
        );
        assert!(
            output.contains("describe"),
            "output should show the spanned source text, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_secondary_span_rendered() {
        // Secondary label should appear in the output alongside the primary.
        let source = "graph G {\n  node A {}\n  node A {}\n}";
        let error = ParseError::new("duplicate node name 'A'", Span::new(23, 29, 3, 3))
            .with_secondary("first defined here", Span::new(12, 18, 2, 3));
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("duplicate node name 'A'"),
            "output should contain primary error, got:\n{output}"
        );
        assert!(
            output.contains("first defined here"),
            "output should contain secondary label, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_duplicate_node_shows_original() {
        // When parsing a file with a duplicate node, the diagnostic should
        // point to both the duplicate and the original definition.
        use crate::parser::parse::parse;

        let source = "graph G {\n  node Dup {}\n  node Dup {}\n}";
        let err = parse(source).unwrap_err();
        assert!(err.message.contains("duplicate node name"));
        assert!(
            !err.secondary.is_empty(),
            "duplicate node error should have a secondary label"
        );
        assert!(
            err.secondary[0].message.contains("first defined"),
            "secondary should mention first definition, got: {}",
            err.secondary[0].message
        );
    }

    #[test]
    fn diagnostic_unclosed_block_shows_opener() {
        // When a graph or node is unclosed, the diagnostic should point back
        // to the opening '{'.
        use crate::parser::parse::parse;

        let source = "graph G {\n  node A {\n    given something\n";
        let err = parse(source).unwrap_err();
        assert!(
            err.message.contains("unclosed"),
            "error should mention unclosed, got: {}",
            err.message
        );
        assert!(
            !err.secondary.is_empty(),
            "unclosed block error should have a secondary label pointing to opener"
        );
        assert!(
            err.secondary[0].message.contains("opened here"),
            "secondary should mention where block was opened, got: {}",
            err.secondary[0].message
        );
    }

    #[test]
    fn diagnostic_help_text_rendered() {
        // Help text should appear in the rendered output.
        let source = "graph G {}";
        let error = ParseError::new("some error", Span::new(0, 5, 1, 1))
            .with_help("try adding a closing brace '}'");
        let output = render_to_string("test.tast", source, &error);
        assert!(
            output.contains("try adding a closing brace '}'"),
            "output should contain help text, got:\n{output}"
        );
    }

    #[test]
    fn diagnostic_unknown_node_has_help() {
        // When an edge references an unknown node, help text should suggest
        // checking the spelling.
        use crate::parser::parse::parse;

        let source = "graph G {\n  node Login {}\n  Login -> Logut\n}";
        let err = parse(source).unwrap_err();
        assert!(err.message.contains("unknown node"));
        assert!(
            err.help.is_some(),
            "unknown node error should have help text"
        );
        let help = err.help.as_deref().unwrap();
        assert!(
            help.contains("Login"),
            "help should suggest similar node name, got: {help}"
        );
    }

    #[test]
    fn diagnostic_unclosed_brace_has_help() {
        // Unclosed block errors should include help text.
        use crate::parser::parse::parse;

        let source = "graph G {\n  node A {\n";
        let err = parse(source).unwrap_err();
        assert!(err.message.contains("unclosed"));
        assert!(
            err.help.is_some(),
            "unclosed block error should have help text"
        );
    }
}
