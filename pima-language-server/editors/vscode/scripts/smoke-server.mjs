import { spawn } from "node:child_process";
import process from "node:process";

const [command] = process.argv.slice(2);
if (!command) {
    console.error("usage: node scripts/smoke-server.mjs <server-binary>");
    process.exit(2);
}

const child = spawn(command, [], { stdio: ["pipe", "pipe", "inherit"] });
let buffer = Buffer.alloc(0);
let nextId = 1;
const pending = new Map();

child.stdout.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    while (true) {
        const headerEnd = buffer.indexOf("\r\n\r\n");
        if (headerEnd < 0) return;
        const header = buffer.subarray(0, headerEnd).toString("utf8");
        const match = /Content-Length:\s*(\d+)/i.exec(header);
        if (!match) throw new Error(`missing Content-Length in ${header}`);
        const length = Number(match[1]);
        const messageEnd = headerEnd + 4 + length;
        if (buffer.length < messageEnd) return;
        const message = JSON.parse(buffer.subarray(headerEnd + 4, messageEnd).toString("utf8"));
        buffer = buffer.subarray(messageEnd);
        if (message.id !== undefined && pending.has(message.id)) {
            pending.get(message.id)(message);
            pending.delete(message.id);
        }
    }
});

function send(message) {
    const body = Buffer.from(JSON.stringify(message));
    child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    child.stdin.write(body);
}

function request(method, params) {
    const id = nextId++;
    return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => {
            pending.delete(id);
            reject(new Error(`timed out waiting for ${method}`));
        }, 10_000);
        pending.set(id, (message) => {
            clearTimeout(timeout);
            if (message.error) reject(new Error(JSON.stringify(message.error)));
            else resolve(message.result);
        });
        const message = { jsonrpc: "2.0", id, method };
        if (params !== undefined) message.params = params;
        send(message);
    });
}

const initialized = await request("initialize", {
    processId: null,
    rootUri: null,
    capabilities: {}
});
if (initialized?.serverInfo?.name !== "Pima Language Server") {
    throw new Error("bundled server returned unexpected initialization data");
}
send({ jsonrpc: "2.0", method: "initialized", params: {} });
await request("shutdown", undefined);
send({ jsonrpc: "2.0", method: "exit" });
child.stdin.end();

const exitCode = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
        child.kill();
        reject(new Error("server did not exit after shutdown"));
    }, 10_000);
    child.once("exit", (code) => {
        clearTimeout(timeout);
        resolve(code);
    });
});
if (exitCode !== 0) throw new Error(`server exited with code ${exitCode}`);
console.log(`smoke-tested ${command}`);
