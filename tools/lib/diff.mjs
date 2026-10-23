// Use the system's diff implementation for bounded, human-readable text differences.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { run } from "./cli.mjs";
export function unified(a, b, from = "expected", to = "actual") {
    if (a === b) return "";
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-diff-"));
    try {
        fs.writeFileSync(path.join(dir, "a"), a);
        fs.writeFileSync(path.join(dir, "b"), b);
        return run(["diff", "-u", "--label", from, "--label", to, path.join(dir, "a"), path.join(dir, "b")], {
            check: false,
        }).stdout;
    } finally {
        fs.rmSync(dir, { recursive: true, force: true });
    }
}
