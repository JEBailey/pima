const assert = require("node:assert/strict");
const path = require("node:path");
const vscode = require("vscode");

async function waitFor(predicate, message) {
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
        const value = predicate();
        if (value) return value;
        await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error(message);
}

async function run() {
    const extension = vscode.extensions.getExtension("local.pima");
    assert.ok(extension, "Pima extension should be installed in the test host");
    await extension.activate();

    const document = await vscode.workspace.openTextDocument(
        path.resolve(__dirname, "fixture", "main.pima")
    );
    assert.equal(document.languageId, "pima");
    const editor = await vscode.window.showTextDocument(document);
    await editor.edit((builder) => {
        builder.insert(new vscode.Position(0, 0), "val incomplete\n");
    });

    const diagnostics = await waitFor(
        () => {
            const current = vscode.languages.getDiagnostics(document.uri);
            return current.length > 0 ? current : undefined;
        },
        "Pima language server did not publish diagnostics"
    );
    assert.ok(diagnostics.some((diagnostic) => diagnostic.source === "pima"));

    const symbols = await vscode.commands.executeCommand(
        "vscode.executeDocumentSymbolProvider",
        document.uri
    );
    assert.ok(Array.isArray(symbols), "document symbols should be available");
}

module.exports = { run };
