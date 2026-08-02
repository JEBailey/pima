use eframe::egui;
use pima::{Config, Interpreter, Value};

struct Example {
    name: &'static str,
    source: &'static str,
}

const EXAMPLES: &[Example] = &[
    Example {
        name: "Counter",
        source: include_str!("../examples/counter.pima"),
    },
    Example {
        name: "Columns",
        source: include_str!("../examples/columns.pima"),
    },
    Example {
        name: "Styling",
        source: include_str!("../examples/styling.pima"),
    },
];

const DEFAULT_PROGRAM: &str = EXAMPLES[0].source;

fn main() -> eframe::Result {
    eframe::run_native(
        "PIMA + egui",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(PimaEguiApp::default()))),
    )
}

struct PimaEguiApp {
    selected_example: usize,
    source: String,
    interpreter: Interpreter,
    view: Option<Value>,
    diagnostics: Vec<String>,
}

impl Default for PimaEguiApp {
    fn default() -> Self {
        let mut app = Self {
            selected_example: 0,
            source: DEFAULT_PROGRAM.to_owned(),
            interpreter: Interpreter::new(Config::default()),
            view: None,
            diagnostics: Vec::new(),
        };
        app.reload();
        app
    }
}

impl PimaEguiApp {
    fn load_selected_example(&mut self) {
        self.source = EXAMPLES[self.selected_example].source.to_owned();
        self.reload();
    }

    fn reload(&mut self) {
        self.interpreter = Interpreter::new(Config::default());
        let source = format!("{}\n[view :init]\n", self.source);
        let outcome = self.interpreter.run_source("<egui-app>", &source);
        self.accept(outcome);
    }

    fn dispatch(&mut self, event: &str) {
        let outcome = self
            .interpreter
            .run_source("<egui-event>", &format!("[view {event}]\n"));
        self.accept(outcome);
    }

    fn accept(&mut self, outcome: pima::RunOutcome) {
        self.diagnostics = outcome
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect();
        if outcome.is_success() {
            self.view = outcome.value;
        }
    }

    fn render_value(&self, ui: &mut egui::Ui, value: &Value, events: &mut Vec<String>) {
        let Value::List(list) = value else {
            ui.colored_label(egui::Color32::YELLOW, "UI node must be a list");
            return;
        };
        let values = list.to_vec();
        let Some(Value::Symbol(tag)) = values.first() else {
            ui.colored_label(egui::Color32::YELLOW, "UI node must begin with a symbol");
            return;
        };
        let Some(tag) = self.interpreter.symbol_name(*tag) else {
            return;
        };

        match tag {
            "column" => {
                ui.vertical(|ui| {
                    for child in &values[1..] {
                        self.render_value(ui, child, events);
                    }
                });
            }
            "row" => {
                ui.horizontal(|ui| {
                    for child in &values[1..] {
                        self.render_value(ui, child, events);
                    }
                });
            }
            "columns" => {
                let children = &values[1..];
                if !children.is_empty() {
                    ui.columns(children.len(), |columns| {
                        for (column, child) in columns.iter_mut().zip(children) {
                            self.render_value(column, child, events);
                        }
                    });
                }
            }
            "heading" => {
                if let Some(Value::String(text)) = values.get(1) {
                    ui.heading(text.as_ref());
                }
            }
            "label" => {
                if let Some(Value::String(text)) = values.get(1) {
                    ui.label(text.as_ref());
                }
            }
            "styled_text" => {
                if let Some(Value::String(text)) = values.get(1) {
                    let mut text = egui::RichText::new(text.as_ref());
                    for style in &values[2..] {
                        match style {
                            Value::Symbol(style) => match self.interpreter.symbol_name(*style) {
                                Some("heading") => text = text.heading(),
                                Some("strong") => text = text.strong(),
                                Some("monospace") => text = text.monospace(),
                                Some("italics") => text = text.italics(),
                                Some("underline") => text = text.underline(),
                                _ => {}
                            },
                            Value::List(style) => {
                                let style = style.to_vec();
                                if list_tag(&self.interpreter, &style) == Some("color")
                                    && let Some(color) = color_from(&style[1..])
                                {
                                    text = text.color(color);
                                }
                            }
                            _ => {}
                        }
                    }
                    ui.label(text);
                }
            }
            "frame" => {
                let mut frame = egui::Frame::new();
                let mut first_child = 1;
                for (index, option) in values[1..].iter().enumerate() {
                    let Value::List(option) = option else {
                        break;
                    };
                    let option = option.to_vec();
                    match list_tag(&self.interpreter, &option) {
                        Some("fill") => {
                            if let Some(color) = color_from(&option[1..]) {
                                frame = frame.fill(color);
                            }
                        }
                        Some("stroke") => {
                            if let Some(color) = color_from(&option[1..]) {
                                frame = frame.stroke(egui::Stroke::new(1.0_f32, color));
                            }
                        }
                        Some("rounding") => {
                            if let Some(Value::Integer(radius)) = option.get(1) {
                                frame = frame.corner_radius((*radius).clamp(0, 255) as u8);
                            }
                        }
                        Some("padding") => {
                            if let Some(Value::Integer(padding)) = option.get(1) {
                                frame = frame.inner_margin((*padding).clamp(0, 127) as i8);
                            }
                        }
                        _ => break,
                    }
                    first_child = index + 2;
                }
                frame.show(ui, |ui| {
                    for child in &values[first_child..] {
                        self.render_value(ui, child, events);
                    }
                });
            }
            "separator" => {
                ui.separator();
            }
            "button" => {
                if let (Some(Value::Symbol(id)), Some(Value::String(label))) =
                    (values.get(1), values.get(2))
                    && ui.button(label.as_ref()).clicked()
                    && let Some(id) = self.interpreter.symbol_name(*id)
                {
                    events.push(format!("(:click :{id})"));
                }
            }
            "text_edit" => {
                if let (Some(Value::Symbol(id)), Some(Value::String(text))) =
                    (values.get(1), values.get(2))
                {
                    let mut edited = text.to_string();
                    if ui.text_edit_singleline(&mut edited).changed()
                        && let Some(id) = self.interpreter.symbol_name(*id)
                    {
                        events.push(format!("(:change :{id} {})", pima_string_literal(&edited)));
                    }
                }
            }
            unknown => {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("unknown PIMA UI node :{unknown}"),
                );
            }
        }
    }
}

fn pima_string_literal(text: &str) -> String {
    let mut literal = String::from("\"");
    for character in text.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            '\0' => literal.push_str("\\0"),
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}

fn list_tag<'a>(interpreter: &'a Interpreter, values: &[Value]) -> Option<&'a str> {
    let Value::Symbol(tag) = values.first()? else {
        return None;
    };
    interpreter.symbol_name(*tag)
}

fn color_from(values: &[Value]) -> Option<egui::Color32> {
    let [
        Value::Integer(red),
        Value::Integer(green),
        Value::Integer(blue),
    ] = values
    else {
        return None;
    };
    Some(egui::Color32::from_rgb(
        (*red).clamp(0, 255) as u8,
        (*green).clamp(0, 255) as u8,
        (*blue).clamp(0, 255) as u8,
    ))
}

impl eframe::App for PimaEguiApp {
    fn update(&mut self, context: &egui::Context, _: &mut eframe::Frame) {
        egui::SidePanel::left("source")
            .resizable(true)
            .default_width(420.0)
            .show(context, |ui| {
                ui.heading("PIMA source");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("example-picker")
                        .selected_text(EXAMPLES[self.selected_example].name)
                        .show_ui(ui, |ui| {
                            for (index, example) in EXAMPLES.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.selected_example,
                                    index,
                                    example.name,
                                );
                            }
                        });
                    if ui.button("Load example").clicked() {
                        self.load_selected_example();
                    }
                });
                if ui.button("Reload").clicked() {
                    self.reload();
                }
                ui.add(
                    egui::TextEdit::multiline(&mut self.source)
                        .code_editor()
                        .desired_rows(30),
                );
                for diagnostic in &self.diagnostics {
                    ui.colored_label(egui::Color32::LIGHT_RED, diagnostic);
                }
            });

        let mut events = Vec::new();
        egui::CentralPanel::default().show(context, |ui| {
            if let Some(view) = &self.view {
                self.render_value(ui, view, &mut events);
            }
        });
        for event in events {
            self.dispatch(&event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_program_returns_a_widget_tree_and_handles_events() {
        let mut interpreter = Interpreter::default();
        let loaded =
            interpreter.run_source("<test-app>", &format!("{DEFAULT_PROGRAM}\n[view :init]\n"));
        assert!(loaded.is_success(), "{:?}", loaded.diagnostics);
        assert!(matches!(loaded.value, Some(Value::List(_))));

        let clicked = interpreter.run_source("<test-event>", "[view (:click :increment)]\n");
        assert!(clicked.is_success(), "{:?}", clicked.diagnostics);
        assert!(matches!(clicked.value, Some(Value::List(_))));
    }

    #[test]
    fn every_bundled_example_returns_a_widget_tree() {
        for example in EXAMPLES {
            let mut interpreter = Interpreter::default();
            let outcome = interpreter
                .run_source(example.name, &format!("{}\n[view :init]\n", example.source));
            assert!(
                outcome.is_success(),
                "{}: {:?}",
                example.name,
                outcome.diagnostics
            );
            assert!(
                matches!(outcome.value, Some(Value::List(_))),
                "{}",
                example.name
            );
        }
    }

    #[test]
    fn event_strings_are_escaped_as_pima_source() {
        assert_eq!(pima_string_literal("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
