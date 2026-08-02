import { copyFile, mkdir } from "node:fs/promises";
import path from "node:path";

await mkdir("dist", { recursive: true });

for (const name of ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE"]) {
    await copyFile(path.join("..", "..", "..", name), name);
}
