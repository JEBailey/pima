# PIMA + egui prototype

This test crate treats the value returned by a PIMA `view` function as a
declarative egui widget tree. Widget and event tags are symbols; text remains
ordinary PIMA strings.

Run it from the workspace root:

```text
cargo run -p pima-egui
```

Use the example picker to load the bundled counter, column-layout, and styling
samples. The counter demonstrates persistent PIMA state and UI events:

```pima
(:column
    (:heading "PIMA creates this egui interface")
    (:row
        (:button :decrement "−")
        (:button :increment "+")
    )
)
```

The host currently understands `:column`, `:row`, `:columns`, `:heading`,
`:label`, `:styled_text`, `:frame`, `:separator`, `:button`, and `:text_edit`.
Styled text accepts `:heading`, `:strong`, `:monospace`, `:italics`,
`:underline`, and `(:color red green blue)`. Frames accept `:fill`, `:stroke`,
`:rounding`, and `:padding` option lists. A button click is passed back to
PIMA as `(:click :button_id)`; edited text is sent as
`(:change :field_id "value")`. The `view` function handles the event and
returns the next widget tree.
