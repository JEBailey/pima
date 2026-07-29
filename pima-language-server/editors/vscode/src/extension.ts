import * as path from "path";
import * as vscode from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const configuredPath = vscode.workspace
        .getConfiguration("pima.languageServer")
        .get<string>("path", "");
    const executable = process.platform === "win32"
        ? "pima-language-server.exe"
        : "pima-language-server";
    const command = configuredPath || path.join(
        context.extensionPath,
        "..",
        "..",
        "..",
        "target",
        "debug",
        executable
    );

    const serverOptions: ServerOptions = { command };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "pima" }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher("**/*.pima")
        }
    };

    client = new LanguageClient(
        "pimaLanguageServer",
        "Pima Language Server",
        serverOptions,
        clientOptions
    );

    await client.start();
}

export async function deactivate(): Promise<void> {
    await client?.stop();
}
