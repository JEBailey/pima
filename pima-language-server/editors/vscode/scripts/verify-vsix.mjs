import { stat } from "node:fs/promises";
import process from "node:process";
import AdmZip from "adm-zip";

const [archive, platform] = process.argv.slice(2);
if (!archive || !platform) {
    console.error("usage: node scripts/verify-vsix.mjs <archive.vsix> <platform>");
    process.exit(2);
}

await stat(archive);
const executable = platform.startsWith("win32-")
    ? "pima-language-server.exe"
    : "pima-language-server";
const required = [
    "extension/package.json",
    "extension/out/extension.js",
    "extension/images/icon.png",
    "extension/syntaxes/pima.tmLanguage.json",
    "extension/LICENSE-MIT",
    "extension/LICENSE-APACHE",
    `extension/server/${platform}/${executable}`
];
const zip = new AdmZip(archive);
const entries = new Set(zip.getEntries().map((entry) => entry.entryName));
const missing = required.filter((entry) => !entries.has(entry));
if (missing.length > 0) {
    throw new Error(`VSIX is missing required entries:\n${missing.join("\n")}`);
}

const manifest = JSON.parse(zip.readAsText("extension/package.json"));
if (manifest.main !== "./out/extension.js" || manifest.icon !== "images/icon.png") {
    throw new Error("VSIX manifest does not reference the packaged client and icon");
}
console.log(`verified ${archive} for ${platform} (${entries.size} entries)`);
