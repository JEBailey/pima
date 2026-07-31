use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{ChildStdin, ChildStdout, Command, Stdio},
};

use serde_json::{Value, json};
use tower_lsp::lsp_types::Url;

fn send(stdin: &mut ChildStdin, message: Value) {
    let body = serde_json::to_vec(&message).expect("serialize request");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
    stdin.write_all(&body).expect("write body");
    stdin.flush().expect("flush request");
}

fn read_message(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        stdout.read_line(&mut header).expect("read header");
        assert!(!header.is_empty(), "language server closed stdout");
        if header == "\r\n" {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>().expect("content length"));
        }
    }
    let mut body = vec![0; content_length.expect("Content-Length header")];
    stdout.read_exact(&mut body).expect("read response body");
    serde_json::from_slice(&body).expect("JSON response")
}

fn response(stdout: &mut BufReader<ChildStdout>, id: i64) -> Value {
    loop {
        let message = read_message(stdout);
        if message.get("id").and_then(Value::as_i64) == Some(id) {
            return message;
        }
    }
}

#[test]
fn serves_editor_features_over_json_rpc() {
    let fixture = std::fs::canonicalize(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workspace"),
    )
    .expect("fixture workspace");
    let root_uri = Url::from_directory_path(&fixture).expect("root URI");
    let main_path = fixture.join("main.pima");
    let main_uri = Url::from_file_path(&main_path).expect("main URI");
    let main_text = std::fs::read_to_string(&main_path).expect("main fixture");

    let mut child = Command::new(env!("CARGO_BIN_EXE_pima-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start language server");
    let mut stdin = child.stdin.take().expect("server stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("server stdout"));

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {}
            }
        }),
    );
    let initialized = response(&mut stdout, 1);
    assert!(
        initialized["result"]["capabilities"]["semanticTokensProvider"].is_object(),
        "semantic token capability missing: {initialized}"
    );
    assert_eq!(
        initialized["result"]["capabilities"]["documentFormattingProvider"],
        true
    );

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": main_uri,
                    "languageId": "pima",
                    "version": 1,
                    "text": main_text
                }
            }
        }),
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": {"uri": main_uri},
                "position": {"line": 2, "character": 8}
            }
        }),
    );
    let completion = response(&mut stdout, 2);
    assert!(
        completion["result"]
            .as_array()
            .expect("completion array")
            .iter()
            .any(|item| item["label"] == "greet")
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/definition",
            "params": {
                "textDocument": {"uri": main_uri},
                "position": {"line": 2, "character": 10}
            }
        }),
    );
    let definition = response(&mut stdout, 3);
    assert!(
        definition["result"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with("library.pima"))
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/references",
            "params": {
                "textDocument": {"uri": main_uri},
                "position": {"line": 2, "character": 10},
                "context": {"includeDeclaration": true}
            }
        }),
    );
    let references = response(&mut stdout, 7);
    let references = references["result"].as_array().expect("references array");
    assert_eq!(references.len(), 2);
    assert!(references.iter().any(|location| {
        location["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with("library.pima"))
    }));

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "textDocument/rename",
            "params": {
                "textDocument": {"uri": main_uri},
                "position": {"line": 2, "character": 10},
                "newName": "welcome"
            }
        }),
    );
    let rename = response(&mut stdout, 8);
    assert_eq!(
        rename["result"]["changes"]
            .as_object()
            .expect("workspace changes")
            .len(),
        2
    );

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/semanticTokens/full",
            "params": {"textDocument": {"uri": main_uri}}
        }),
    );
    assert!(
        response(&mut stdout, 5)["result"]["data"]
            .as_array()
            .is_some_and(|data| !data.is_empty())
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": {"uri": main_uri},
                "options": {"tabSize": 4, "insertSpaces": true}
            }
        }),
    );
    assert!(response(&mut stdout, 6)["result"].is_array());

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": main_uri, "version": 3},
                "contentChanges": [{"text": "val :newest 1\nnewest\n"}]
            }
        }),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {"uri": main_uri, "version": 2},
                "contentChanges": [{"text": "val :stale 1\nstale\n"}]
            }
        }),
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/completion",
            "params": {
                "textDocument": {"uri": main_uri},
                "position": {"line": 1, "character": 6}
            }
        }),
    );
    let completion = response(&mut stdout, 4);
    let labels = completion["result"]
        .as_array()
        .expect("completion array")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"newest"));
    assert!(!labels.contains(&"stale"));

    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}),
    );
    assert!(response(&mut stdout, 99)["result"].is_null());
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "exit", "params": null}),
    );
    drop(stdin);
    assert!(child.wait().expect("server exit").success());
}
