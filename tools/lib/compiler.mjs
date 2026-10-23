import path from "node:path";
import { ROOT, requireValue } from "./cli.mjs";
export function referenceBinary() {
    const exe = process.env.TSGO ?? (process.env.TSRS_WORK ? path.join(process.env.TSRS_WORK, "bin/tsgo-ref") : null);
    requireValue(exe, "set TSGO to the reference tsgo binary");
    return exe;
}
export const tsrsBinary = () => process.env.TSRS ?? path.join(ROOT, "target/release/tsrs");
