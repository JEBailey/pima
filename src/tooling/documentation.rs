use crate::syntax::ast::{Module, NodeId, NodeKind, Pattern, Visibility};

#[derive(Clone, Debug, PartialEq)]
pub struct Documentation {
    pub module: String,
    pub description: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    pub signature: String,
    pub description: String,
    pub members: Vec<Item>,
}

pub fn extract(path: &std::path::Path, source: &str, module: &Module) -> Documentation {
    Documentation {
        module: path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Pima module")
            .to_owned(),
        description: module_documentation(source).unwrap_or_default(),
        items: module
            .statements
            .iter()
            .filter_map(|statement| document_node(source, module, *statement))
            .collect(),
    }
}

pub fn markdown(documentation: &Documentation) -> String {
    let mut output = format!("# {}\n\n", documentation.module);
    if !documentation.description.is_empty() {
        output.push_str(&documentation.description);
        output.push_str("\n\n");
    }
    for item in &documentation.items {
        markdown_item(item, 2, &mut output);
    }
    output
}

pub fn html(documentation: &Documentation, modules: &[String]) -> String {
    let title = escape_html(&documentation.module);
    let mut navigation = String::new();
    for module in modules {
        navigation.push_str(&format!(
            "<a href=\"{}.html\">{}</a>",
            escape_attribute(module),
            escape_html(module)
        ));
    }
    let mut content = String::new();
    if !documentation.description.is_empty() {
        content.push_str(&paragraphs(&documentation.description));
    }
    for item in &documentation.items {
        html_item(item, 2, &mut content);
    }
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title} - Pima documentation</title><link rel=\"stylesheet\" href=\"style.css\"></head><body><aside><a class=\"home\" href=\"index.html\">Pima documentation</a><nav>{navigation}</nav></aside><main><h1>{title}</h1>{content}</main></body></html>\n"
    )
}

pub fn json(documentation: &Documentation) -> String {
    serde_json::to_string_pretty(&json_documentation(documentation))
        .expect("documentation contains only serializable strings")
        + "\n"
}

pub fn index_html(documents: &[Documentation]) -> String {
    let mut modules = String::new();
    for document in documents {
        modules.push_str(&format!(
            "<li><a href=\"{}.html\"><code>{}</code></a>{}</li>",
            escape_attribute(&document.module),
            escape_html(&document.module),
            if document.description.is_empty() {
                String::new()
            } else {
                format!(
                    " — {}",
                    escape_html(document.description.lines().next().unwrap_or(""))
                )
            }
        ));
    }
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Pima documentation</title><link rel=\"stylesheet\" href=\"style.css\"></head><body><aside><span class=\"home\">Pima documentation</span></aside><main><h1>Modules</h1><ul class=\"modules\">{modules}</ul></main></body></html>\n"
    )
}

pub const STYLE: &str = r#":root{font-family:Inter,ui-sans-serif,system-ui,sans-serif;color:#202124;background:#fff}*{box-sizing:border-box}body{margin:0;display:grid;grid-template-columns:16rem minmax(0,1fr);min-height:100vh}aside{padding:1.5rem;background:#f6f7f9;border-right:1px solid #ddd}nav{display:flex;flex-direction:column;margin-top:1.25rem;gap:.45rem}a{color:#3658a7;text-decoration:none}a:hover{text-decoration:underline}.home{font-weight:700;color:#202124}main{width:min(56rem,100%);padding:3rem 4rem}h1{font-size:2.25rem}h2{margin-top:2.5rem;border-bottom:1px solid #ddd;padding-bottom:.4rem}h3{margin-top:2rem}p{line-height:1.65}pre{overflow:auto;padding:1rem 1.2rem;border-radius:.4rem;background:#f2f3f5;border:1px solid #ddd}code{font-family:ui-monospace,SFMono-Regular,Consolas,monospace}.modules{line-height:2}@media(max-width:720px){body{display:block}aside{border-right:0;border-bottom:1px solid #ddd}nav{flex-direction:row;flex-wrap:wrap}main{padding:2rem 1.25rem}}"#;

fn document_node(source: &str, module: &Module, id: NodeId) -> Option<Item> {
    let node = module.node(id);
    match &node.kind {
        NodeKind::Binding {
            visibility: Visibility::Public,
            pattern,
            value,
            ..
        } => {
            let name = pattern_name(pattern)?.to_owned();
            let members = match module.node(*value).kind {
                NodeKind::Block(block) => module
                    .block(block)
                    .statements
                    .iter()
                    .filter_map(|statement| document_node(source, module, *statement))
                    .collect(),
                _ => Vec::new(),
            };
            Some(Item {
                name,
                signature: signature(source, node.span.start),
                description: documentation_before(source, node.span.start),
                members,
            })
        }
        NodeKind::Function {
            visibility: Visibility::Public,
            name,
            ..
        } => Some(Item {
            name: name.text.to_string(),
            signature: signature(source, node.span.start),
            description: documentation_before(source, node.span.start),
            members: Vec::new(),
        }),
        _ => None,
    }
}

fn markdown_item(item: &Item, level: usize, output: &mut String) {
    output.push_str(&"#".repeat(level));
    output.push(' ');
    output.push_str(&item.name);
    output.push_str("\n\n");
    if !item.description.is_empty() {
        output.push_str(&item.description);
        output.push_str("\n\n");
    }
    output.push_str("```pima\n");
    output.push_str(&item.signature);
    output.push_str("\n```\n\n");
    for member in &item.members {
        markdown_item(member, level + 1, output);
    }
}

fn html_item(item: &Item, level: usize, output: &mut String) {
    let level = level.min(6);
    output.push_str(&format!(
        "<section id=\"{}\"><h{level}>{}</h{level}>",
        escape_attribute(&item.name),
        escape_html(&item.name)
    ));
    if !item.description.is_empty() {
        output.push_str(&paragraphs(&item.description));
    }
    output.push_str("<pre><code>");
    output.push_str(&escape_html(&item.signature));
    output.push_str("</code></pre>");
    for member in &item.members {
        html_item(member, level + 1, output);
    }
    output.push_str("</section>");
}

fn json_documentation(documentation: &Documentation) -> serde_json::Value {
    serde_json::json!({
        "module": documentation.module,
        "description": documentation.description,
        "items": documentation.items.iter().map(json_item).collect::<Vec<_>>()
    })
}

fn json_item(item: &Item) -> serde_json::Value {
    serde_json::json!({
        "name": item.name,
        "signature": item.signature,
        "description": item.description,
        "members": item.members.iter().map(json_item).collect::<Vec<_>>()
    })
}

fn signature(source: &str, start: usize) -> String {
    let line = source[start..].lines().next().unwrap_or("").trim();
    line.strip_suffix('{')
        .map(str::trim_end)
        .unwrap_or(line)
        .to_owned()
}

fn pattern_name(pattern: &Pattern) -> Option<&str> {
    match pattern {
        Pattern::Capture(name) => Some(&name.text),
        _ => None,
    }
}

fn module_documentation(source: &str) -> Option<String> {
    let lines = source
        .lines()
        .take_while(|line| line.trim().is_empty() || line.trim_start().starts_with("//!"))
        .filter_map(|line| line.trim_start().strip_prefix("//!"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn documentation_before(source: &str, start: usize) -> String {
    let mut documentation = Vec::new();
    for line in source[..start].lines().collect::<Vec<_>>().iter().rev() {
        let trimmed = line.trim_start();
        if let Some(text) = trimmed.strip_prefix("///") {
            documentation.push(text.strip_prefix(' ').unwrap_or(text));
        } else {
            break;
        }
    }
    documentation.reverse();
    documentation.join("\n")
}

fn paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(|paragraph| format!("<p>{}</p>", escape_html(paragraph).replace('\n', "<br>")))
        .collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_attribute(value: &str) -> String {
    escape_html(value).replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use crate::{
        source::SourceMap,
        syntax::{lexer::lex, parser::parse},
    };

    fn documentation() -> super::Documentation {
        let source = "//! Counters.\n\n/// Current count.\npub val :count 0\n";
        let mut sources = SourceMap::default();
        let id = sources.add("counter.pima", source);
        let module = parse(&lex(id, source).unwrap()).unwrap();
        super::extract(std::path::Path::new("counter.pima"), source, &module)
    }

    #[test]
    fn renders_all_formats() {
        let documentation = documentation();
        assert!(super::markdown(&documentation).contains("## count"));
        assert!(super::html(&documentation, &["counter".into()]).contains("<!doctype html>"));
        let json: serde_json::Value = serde_json::from_str(&super::json(&documentation)).unwrap();
        assert_eq!(json["module"], "counter");
    }
}
