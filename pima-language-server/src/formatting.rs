pub fn format(source: &str, indent_width: usize) -> Option<String> {
    pima::tooling::formatting::format(source, indent_width).ok()
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
    fn declines_to_format_invalid_lexical_input() {
        assert!(format("\"unterminated", 4).is_none());
    }
}
