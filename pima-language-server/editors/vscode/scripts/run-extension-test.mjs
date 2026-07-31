import path from "node:path";
import { runTests } from "@vscode/test-electron";

await runTests({
    extensionDevelopmentPath: path.resolve("."),
    extensionTestsPath: path.resolve("test", "index.cjs"),
    launchArgs: [path.resolve("test", "fixture")]
});
