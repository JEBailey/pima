import { chmod, copyFile, mkdir, stat } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const supported = new Set(["win32-x64", "linux-x64", "darwin-x64", "darwin-arm64"]);
const [platform, source] = process.argv.slice(2);

if (!supported.has(platform) || !source) {
    console.error("usage: node scripts/stage-server.mjs <platform> <server-binary>");
    process.exit(2);
}

const sourceStats = await stat(source).catch(() => undefined);
if (!sourceStats?.isFile()) {
    throw new Error(`server binary does not exist: ${source}`);
}

const executable = platform.startsWith("win32-")
    ? "pima-language-server.exe"
    : "pima-language-server";
const destinationDirectory = path.join("server", platform);
const destination = path.join(destinationDirectory, executable);
await mkdir(destinationDirectory, { recursive: true });
await copyFile(source, destination);
if (!platform.startsWith("win32-")) {
    await chmod(destination, 0o755);
}
console.log(destination);
