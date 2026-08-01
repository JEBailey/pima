use crate::{
    source::SourceMap,
    syntax::{lexer::lex, token::TokenKind},
};

/// Conservatively formats indentation without changing Pima's semantic line
/// boundaries.
pub fn format(source: &str, indent_width: usize) -> Result<String, Vec<crate::Diagnostic>> {
    let mut sources = SourceMap::default();
    let source_id = sources.add("<format>", source);
    let tokens = lex(source_id, source)?;
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
            output.push_str(&" ".repeat(depth.saturating_sub(leading_closers) * indent_width));
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
    Ok(output)
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
    fn preserves_lines_and_formats_indentation() {
        let source = "if true {\nvalue\n}\n";
        assert_eq!(format(source, 4).unwrap(), "if true {\n    value\n}\n");
    }
}
