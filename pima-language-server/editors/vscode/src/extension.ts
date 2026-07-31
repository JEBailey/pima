import * as path from "path";
import * as fs from "fs";
import * as vscode from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let output: vscode.LogOutputChannel | undefined;

function platformDirectory(): string {
    const supported = new Set([
        "win32-x64",
        "linux-x64",
        "darwin-x64",
        "darwin-arm64"
    ]);
    const target = `${process.platform}-${process.arch}`;
    if (!supported.has(target)) {
        throw new Error(
            `Pima does not currently bundle a language server for ${target}. ` +
            "Configure pima.languageServer.path to use a compatible executable."
        );
    }
    return target;
}

function executableName(): string {
    return process.platform === "win32"
        ? "pima-language-server.exe"
        : "pima-language-server";
}

function developmentServer(context: vscode.ExtensionContext): string {
    return path.resolve(
        context.extensionPath,
        "..",
        "..",
        "..",
        "target",
        "debug",
        executableName()
    );
}

function resolveServer(context: vscode.ExtensionContext): string {
    const configured = vscode.workspace
        .getConfiguration("pima.languageServer")
        .get<string>("path", "")
        .trim();
    if (configured) {
        return path.resolve(configured);
    }

    const bundled = context.asAbsolutePath(path.join(
        "server",
        platformDirectory(),
        executableName()
    ));
    if (fs.existsSync(bundled)) {
        return bundled;
    }
    if (context.extensionMode === vscode.ExtensionMode.Development) {
        return developmentServer(context);
    }
    throw new Error(
        `The bundled Pima language server is missing: ${bundled}. ` +
        "Reinstall the extension or configure pima.languageServer.path."
    );
}

function verifyServer(command: string): void {
    let stats: fs.Stats;
    try {
        stats = fs.statSync(command);
    } catch (error) {
        throw new Error(`Pima language server was not found at ${command}: ${String(error)}`);
    }
    if (!stats.isFile()) {
        throw new Error(`Pima language server path is not a file: ${command}`);
    }
    if (process.platform !== "win32") {
        try {
            fs.accessSync(command, fs.constants.X_OK);
        } catch {
            throw new Error(`Pima language server is not executable: ${command}`);
        }
    }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    output = vscode.window.createOutputChannel("Pima", { log: true });
    context.subscriptions.push(output);

    let command: string;
    try {
        command = resolveServer(context);
        verifyServer(command);
    } catch (error) {
        const message = `Pima language server could not start: ${String(error)}`;
        output.appendLine(message);
        const choice = await vscode.window.showErrorMessage(message, "Show Pima Output");
        if (choice === "Show Pima Output") {
            output.show(true);
        }
        return;
    }

    output.appendLine(`Starting Pima language server: ${command}`);

    const serverOptions: ServerOptions = {
        run: { command },
        debug: { command }
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "pima" }],
        outputChannel: output,
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

    try {
        await client.start();
        output.appendLine("Pima language server started.");
    } catch (error) {
        const message = `Pima language server failed to start: ${String(error)}`;
        output.appendLine(message);
        const choice = await vscode.window.showErrorMessage(message, "Show Pima Output");
        if (choice === "Show Pima Output") {
            output.show(true);
        }
        client = undefined;
    }
}

export async function deactivate(): Promise<void> {
    await client?.stop();
    client = undefined;
}
