import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import getExePath from "./getExePath.js";

// VS Code starts tsgo.exe directly on Windows. Keep its default libraries beside it,
// as in the platform package, so noembed builds resolve them without TSRS_LIB_PATH.
export function setupWindowsSdk() {
    const exe = getExePath();
    const sourceDir = path.dirname(exe);
    const sdkDir = fileURLToPath(new URL("../tsdk/", import.meta.url));
    const isLib = name => name.startsWith("lib.") && name.endsWith(".d.ts");
    const libs = fs.readdirSync(sourceDir).filter(isLib);
    fs.mkdirSync(sdkDir, { recursive: true });
    for (const name of fs.readdirSync(sdkDir)) {
        if (isLib(name) && !libs.includes(name)) fs.unlinkSync(path.join(sdkDir, name));
    }
    for (const name of libs) installFile(path.join(sourceDir, name), path.join(sdkDir, name));
    installFile(exe, path.join(sdkDir, "tsgo.exe"));
}

function installFile(source, target) {
    const src = fs.statSync(source, { bigint: true });
    if (fs.existsSync(target)) {
        const dst = fs.statSync(target, { bigint: true });
        if (src.dev === dst.dev && src.ino === dst.ino) return;
        fs.unlinkSync(target);
    }
    try {
        fs.linkSync(source, target);
    }
    catch {
        // Package stores can live on another volume or disallow hard links.
        fs.copyFileSync(source, target);
    }
}

if (process.platform === "win32") {
    try {
        setupWindowsSdk();
    }
    catch (error) {
        console.error(`tsrs: could not prepare the Windows VS Code SDK: ${error.message}\n` +
            "Close any running tsrs language server, then run node node_modules/tsrs/lib/setupTsdk.js to retry.");
        process.exitCode = 1;
    }
}
