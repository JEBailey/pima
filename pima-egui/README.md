# PIMA + egui prototype

This test crate treats the value returned by a PIMA `view` function as a
declarative egui widget tree. Widget and event tags are symbols; text remains
ordinary PIMA strings.

Run it from the workspace root:

```text
cargo run -p pima-egui
```

The included counter demonstrates persistent PIMA state and UI events:

```pima
(:column
    (:heading "PIMA creates this egui interface")
    (:row
        (:button :decrement "−")
        (:button :increment "+")
    )
)
```

The host currently understands `:column`, `:row`, `:heading`, `:label`,
`:separator`, `:button`, and `:text_edit`. A button click is passed back to
PIMA as `(:click :button_id)`; edited text is sent as
`(:change :field_id "value")`. The `view` function handles the event and
returns the next widget tree.
