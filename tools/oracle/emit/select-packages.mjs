#!/usr/bin/env node
// Select package directories whose build script invokes a compiler directly.
import fs from "node:fs";
import path from "node:path";
import { json, args, requireValue, main } from "../../lib/cli.mjs";
main(() => {
    const a = args("Usage: select-packages <repository> [filter regexp]");
    requireValue(a._[0], "repository is required");
    const root = path.resolve(a._[0]),
        filter = a._[1] ? new RegExp(a._[1]) : null;
    for (const sub of ["apps", "packages"]) {
        const dir = path.join(root, sub);
        if (!fs.existsSync(dir)) continue;
        for (const name of fs.readdirSync(dir).sort()) {
            const rel = sub + "/" + name,
                pkg = path.join(dir, name, "package.json");
            if (!fs.existsSync(pkg) || (filter && !filter.test(rel))) continue;
            for (const cmd of (json(pkg).scripts?.build ?? "").split(/&&|;|\|\|/)) {
                const words = cmd.trim().split(/\s+/);
                if (!["tsc", "tsgo"].includes(words[0])) continue;
                let config = "tsconfig.json";
                for (let i = 0; i < words.length; i++)
                    if (["-p", "--project"].includes(words[i]) && words[i + 1]) config = words[i + 1];
                let target = path.resolve(root, rel, config);
                if (fs.existsSync(target) && fs.statSync(target).isDirectory())
                    target = path.join(target, "tsconfig.json");
                if (fs.existsSync(target)) console.log(`${rel}\t${target}`);
                break;
            }
        }
    }
});
