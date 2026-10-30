import fs from "node:fs";
import module from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { getPlatformPackageName } from "./platform.js";

// The native binary ships in a per-platform optional dependency, with GNU/musl suffixes on Linux as in Oxc.
export default function getExePath() {
    return platformExePath("tsrs", "TSRS_BINARY");
}

// `tsrs-lint`, tsgolint's headless helper protocol (crates/tsrs_lint), ships beside tsrs in the same package.
export function getLintExePath() {
    return platformExePath("tsrs-lint", "TSRS_LINT_BINARY");
}

function platformExePath(name, overrideVariable) {
    const override = process.env[overrideVariable];
    if (override) {
        if (!fs.existsSync(override)) {
            throw new Error(`${overrideVariable} is set to ${override}, which does not exist.`);
        }
        return override;
    }

    const __dirname = path.dirname(fileURLToPath(import.meta.url));
    const pkg = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8"));
    const platformPackagePrefix = "@tsrs/";
    const platformPackageName = getPlatformPackageName();
    const exeName = process.platform === "win32" ? `${name}.exe` : name;

    let packageDir;
    try {
        const require = module.createRequire(import.meta.url);
        packageDir = path.dirname(require.resolve(`${platformPackageName}/package.json`));
    }
    catch {
        const supported = Object.keys(pkg.optionalDependencies ?? {});
        const isSupported = supported.includes(platformPackageName);
        throw new Error(
            [
                `Cannot find the optional dependency ${platformPackageName}, which contains the ${name} binary for ${platformPackageName.slice(platformPackagePrefix.length)}.`,
                isSupported
                    ? `It is listed in ${pkg.name}'s optionalDependencies but was not installed. Common causes: installing with --no-optional / --omit=optional, ` +
                        `a lockfile or node_modules created on another platform, or pnpm \`supportedArchitectures\` excluding this platform. Reinstall to fix.`
                    : `This platform is not supported. Supported: ${supported.map(n => n.slice(platformPackagePrefix.length)).join(", ")}.`,
                `To use a locally built binary instead, set ${overrideVariable}=/path/to/${exeName}.`,
            ].join("\n"),
        );
    }

    let exe = path.join(packageDir, exeName);
    if (process.platform === "win32" && exe.length >= 248) {
        exe = "\\\\?\\" + exe;
    }
    if (!fs.existsSync(exe)) {
        throw new Error(`${platformPackageName} is installed at ${packageDir} but has no ${exeName}.`);
    }
    return exe;
}
