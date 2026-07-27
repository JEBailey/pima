use super::{Diagnostic, Severity};
use crate::source::SourceMap;

pub fn render(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let label = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let mut rendered = format!("{label}: {}", diagnostic.message);
    if let Some(span) = diagnostic.primary_span
        && let Some(source) = sources.get(span.source)
        && let Some((line, column)) = sources.line_column(span.source, span.start)
    {
        rendered.push_str(&format!("\n  at {}:{line}:{column}", source.name));
    }
    for frame in &diagnostic.stack {
        if let Some(source) = sources.get(frame.span.source)
            && let Some((line, column)) = sources.line_column(frame.span.source, frame.span.start)
        {
            rendered.push_str(&format!(
                "\n  in {} at {}:{line}:{column}",
                frame.name, source.name
            ));
        }
    }
    rendered
}
