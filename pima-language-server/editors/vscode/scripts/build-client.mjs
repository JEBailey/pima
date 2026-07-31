import path from "node:path";
import process from "node:process";
import { build, context } from "esbuild";

const options = {
    entryPoints: [path.resolve("src", "extension.ts")],
    outfile: path.resolve("out", "extension.js"),
    bundle: true,
    platform: "node",
    format: "cjs",
    external: ["vscode"],
    logLevel: "info"
};

if (process.argv.includes("--watch")) {
    const buildContext = await context(options);
    await buildContext.watch();
    console.log("watching Pima extension sources");
} else {
    await build(options);
}
