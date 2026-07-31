use pima::{
    source::SourceMap,
    syntax::{lexer::lex, token::TokenKind},
};

pub fn format(source: &str, indent_width: usize) -> Option<String> {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<format>", source);
    let tokens = lex(source_id, source).ok()?;
    let mut output = String::new();
    let mut depth = 0_usize;
    let mut offset = 0;

    for line_with_ending in source.split_inclusive('\n') {
        let line = line_with_ending
            .strip_suffix('\n')
            .unwrap_or(line_with_ending)
            .strip_suffix('\r')
            .unwrap_or_else(|| {
                line_with_ending
                    .strip_suffix('\n')
                    .unwrap_or(line_with_ending)
            });
        let line_end = offset + line.len();
        let line_tokens = tokens
            .iter()
            .filter(|token| {
                token.span.start >= offset
                    && token.span.start <= line_end
                    && !matches!(token.kind, TokenKind::Eol | TokenKind::Eof)
            })
            .collect::<Vec<_>>();
        let content = line.trim();
        if !content.is_empty() {
            let leading_closers = line_tokens
                .iter()
                .take_while(|token| is_closer(&token.kind))
                .count();
            let indentation = depth.saturating_sub(leading_closers);
            output.push_str(&" ".repeat(indentation * indent_width));
            output.push_str(content);
        }
        if line_with_ending.ends_with('\n') {
            output.push('\n');
        }

        for token in line_tokens {
            if is_opener(&token.kind) {
                depth += 1;
            } else if is_closer(&token.kind) {
                depth = depth.saturating_sub(1);
            }
        }
        offset += line_with_ending.len();
    }

    if !source.is_empty() && !source.ends_with('\n') && output.ends_with('\n') {
        output.pop();
    }
    Some(output)
}

fn is_opener(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::LeftBrace | TokenKind::LeftBracket | TokenKind::LeftParen
    )
}

fn is_closer(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::RightBrace | TokenKind::RightBracket | TokenKind::RightParen
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_nested_blocks_and_preserves_comments() {
        let source = "function :read (value) {\n// comment\nif true {\nvalue   \n} {\n0\n}\n}\n";
        assert_eq!(
            format(source, 4).expect("format"),
            "function :read (value) {\n    // comment\n    if true {\n        value\n    } {\n        0\n    }\n}\n"
        );
    }

    #[test]
    fn formats_multiline_lists_and_calls() {
        let source = "val :values (\n1\n[+ 2\n3]\n)\n";
        assert_eq!(
            format(source, 2).expect("format"),
            "val :values (\n  1\n  [+ 2\n    3]\n)\n"
        );
    }

    #[test]
    fn declines_to_format_invalid_lexical_input() {
        assert!(format("\"unterminated", 4).is_none());
    }

    #[test]
    fn formats_branch_pairs() {
        let source = "val :result branch (\ntrue {\n1\n}\nfalse 2\n)\n";
        assert_eq!(
            format(source, 4).expect("format"),
            "val :result branch (\n    true {\n        1\n    }\n    false 2\n)\n"
        );
    }
}
