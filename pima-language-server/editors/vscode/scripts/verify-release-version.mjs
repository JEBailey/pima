import { readFile } from "node:fs/promises";
import process from "node:process";

async function cargoVersion(file) {
    const source = await readFile(file, "utf8");
    const packageSection = source.match(/\[package\]([\s\S]*?)(?:\n\[|$)/)?.[1];
    const version = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
    if (!version) throw new Error(`could not read package version from ${file}`);
    return version;
}

const extension = JSON.parse(await readFile("package.json", "utf8"));
const versions = new Map([
    ["pima", await cargoVersion("../../../Cargo.toml")],
    ["pima-language-server", await cargoVersion("../../Cargo.toml")],
    ["vscode-extension", extension.version]
]);
const distinct = new Set(versions.values());
if (distinct.size !== 1) {
    throw new Error(`release versions differ: ${JSON.stringify(Object.fromEntries(versions))}`);
}

const version = [...distinct][0];
const tag = process.env.GITHUB_REF_NAME;
const isTagBuild = process.env.GITHUB_REF_TYPE === "tag";
if (isTagBuild && tag !== `v${version}` && !tag?.startsWith(`v${version}-`)) {
    throw new Error(`tag ${tag} does not match package version ${version}`);
}
console.log(`release version ${version}${isTagBuild ? ` matches ${tag}` : " across manifests"}`);
