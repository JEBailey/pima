use ariadne::{Config, Label, Report, ReportKind};

use super::{Diagnostic, Severity};
use crate::source::SourceMap;

pub fn render(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let kind = match diagnostic.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
    };
    let primary = diagnostic
        .primary_span
        .and_then(|span| {
            let source = sources.get(span.source)?;
            Some((source.name.to_string(), span.start..span.end))
        })
        .unwrap_or_else(|| ("<unknown>".to_owned(), 0..0));
    let mut report = Report::build(kind, primary.clone())
        .with_message(&diagnostic.message)
        .with_config(Config::default().with_color(false));
    if diagnostic.primary_span.is_some() {
        report = report.with_label(
            Label::new(primary)
                .with_message(&diagnostic.message)
                .with_priority(1),
        );
    }
    for frame in &diagnostic.stack {
        if let Some(source) = sources.get(frame.span.source) {
            report = report.with_label(
                Label::new((source.name.to_string(), frame.span.start..frame.span.end))
                    .with_message(format!("called from `{}`", frame.name)),
            );
        }
    }
    let mut cache = sources
        .files()
        .iter()
        .map(|source| (source.name.to_string(), source.text.to_string()))
        .collect::<Vec<_>>();
    if cache.is_empty() {
        cache.push(("<unknown>".to_owned(), String::new()));
    }
    let mut output = Vec::new();
    if report
        .finish()
        .write(ariadne::sources(cache), &mut output)
        .is_err()
    {
        return format!("error: {}", diagnostic.message);
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use crate::{
        diagnostic::{Diagnostic, Severity, StackFrame},
        source::{SourceMap, Span},
    };

    use super::render;

    #[test]
    fn syntax_diagnostic_snapshot() {
        let mut sources = SourceMap::default();
        let source = sources.add("example.pima", "val answer [missing]\n");
        let diagnostic =
            Diagnostic::at_error("unbound identifier `missing`", Span::new(source, 12, 19));

        insta::assert_snapshot!(render(&diagnostic, &sources), @r###"
        Error: unbound identifier `missing`
           ╭─[ example.pima:1:13 ]
           │
         1 │ val answer [missing]
           │             ───┬───  
           │                ╰───── unbound identifier `missing`
        ───╯
        "###);
    }

    #[test]
    fn runtime_stack_snapshot() {
        let mut sources = SourceMap::default();
        let source = sources.add(
            "stack.pima",
            "function inner () {\n    missing\n}\n[inner]\n",
        );
        let diagnostic = Diagnostic {
            severity: Severity::Error,
            message: "unbound identifier `missing`".to_owned(),
            primary_span: Some(Span::new(source, 24, 31)),
            stack: vec![StackFrame {
                name: "inner".to_owned(),
                span: Span::new(source, 34, 41),
            }],
        };

        insta::assert_snapshot!(render(&diagnostic, &sources), @r###"
        Error: unbound identifier `missing`
           ╭─[ stack.pima:2:5 ]
           │
         2 │     missing
           │     ───┬───  
           │        ╰───── unbound identifier `missing`
           │ 
         4 │ [inner]
           │ ───┬───  
           │    ╰───── called from `inner`
        ───╯
        "###);
    }
}
