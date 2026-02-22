/// Source location tracking for error reporting.
///
/// A `Span` marks a range of characters in a source file,
/// used to produce helpful error messages with line/column info.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Byte offset of the start of the span.
    pub start: usize,
    /// Byte offset of the end of the span (exclusive).
    pub end: usize,
    /// 1-based line number where the span starts.
    pub line: usize,
    /// 1-based column number where the span starts.
    pub col: usize,
}

impl Span {
    /// Creates a new span with the given byte offsets and source location.
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Self {
            start,
            end,
            line,
            col,
        }
    }

    /// Returns the length of the span in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Returns true if the span has zero length.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merge two spans into one that covers both ranges.
    /// Takes the start of `self` and the end of `other`.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
            line: self.line,
            col: self.col,
        }
    }
}

impl Default for Span {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_span_with_new() {
        let span = Span::new(0, 5, 1, 1);
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 5);
        assert_eq!(span.line, 1);
        assert_eq!(span.col, 1);
    }

    #[test]
    fn span_len_returns_byte_length() {
        let span = Span::new(10, 25, 3, 5);
        assert_eq!(span.len(), 15);
    }

    #[test]
    fn span_is_empty_when_start_equals_end() {
        let span = Span::new(5, 5, 1, 6);
        assert!(span.is_empty());
    }

    #[test]
    fn span_is_not_empty_when_start_differs_from_end() {
        let span = Span::new(0, 1, 1, 1);
        assert!(!span.is_empty());
    }

    #[test]
    fn span_default_is_zero_position_line_one_col_one() {
        let span = Span::default();
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 0);
        assert_eq!(span.line, 1);
        assert_eq!(span.col, 1);
    }

    #[test]
    fn span_merge_combines_two_spans() {
        let a = Span::new(0, 5, 1, 1);
        let b = Span::new(5, 12, 1, 6);
        let merged = a.merge(b);
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 12);
        assert_eq!(merged.line, 1);
        assert_eq!(merged.col, 1);
    }

    #[test]
    fn span_copy_semantics() {
        let a = Span::new(0, 5, 1, 1);
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    #[test]
    fn span_equality() {
        let a = Span::new(0, 5, 1, 1);
        let b = Span::new(0, 5, 1, 1);
        let c = Span::new(0, 5, 2, 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn span_debug_format() {
        let span = Span::new(0, 5, 1, 1);
        let debug = format!("{:?}", span);
        assert!(debug.contains("Span"));
        assert!(debug.contains("start: 0"));
    }
}
