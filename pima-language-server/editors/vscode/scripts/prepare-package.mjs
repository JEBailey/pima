import { copyFile } from "node:fs/promises";
import path from "node:path";

for (const name of ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE"]) {
    await copyFile(path.join("..", "..", "..", name), name);
}
