use eframe::egui;
use pima::{Config, Interpreter, Value};

const DEFAULT_PROGRAM: &str = include_str!("../examples/counter.pima");

fn main() -> eframe::Result {
    eframe::run_native(
        "PIMA + egui",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(PimaEguiApp::default()))),
    )
}

struct PimaEguiApp {
    source: String,
    interpreter: Interpreter,
    view: Option<Value>,
    diagnostics: Vec<String>,
}

impl Default for PimaEguiApp {
    fn default() -> Self {
        let mut app = Self {
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

impl eframe::App for PimaEguiApp {
    fn update(&mut self, context: &egui::Context, _: &mut eframe::Frame) {
        egui::SidePanel::left("source")
            .resizable(true)
            .default_width(420.0)
            .show(context, |ui| {
                ui.heading("PIMA source");
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
    fn event_strings_are_escaped_as_pima_source() {
        assert_eq!(pima_string_literal("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
