// Runs the vendored upstream sync API suites (npm/tsrs/test/sync) with every session in this process, through
// native-sync.mjs. Extra arguments go to `node --test`, for example --test-reporter=dot.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { fileURLToPath } from "node:url";

const sdk = new URL("../../npm/tsrs/", import.meta.url);
const files = fs.readdirSync(new URL("test/sync/", sdk))
    .filter(name => name.endsWith(".test.ts"))
    .map(name => `test/sync/${name}`);
// NODE_OPTIONS also reaches the test file processes and the processes shutdown.test.ts spawns.
const nodeOptions = [process.env.NODE_OPTIONS, "--conditions=@typescript/source", `--import=${new URL("native-sync.mjs", import.meta.url).href}`];
const result = spawnSync(process.execPath, ["--test", ...process.argv.slice(2), ...files], {
    cwd: fileURLToPath(sdk),
    stdio: "inherit",
    // A missing TSRS_BINARY makes any `tsrs --api` spawn fail, so a passing run never left the process.
    env: { ...process.env, NODE_OPTIONS: nodeOptions.filter(Boolean).join(" "), TSRS_BINARY: fileURLToPath(new URL("no-tsrs-binary", import.meta.url)) },
});
process.exitCode = result.status ?? 1;
