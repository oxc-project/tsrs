#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { gunzipSync } from "node:zlib";
import assert from "node:assert/strict";
import { ROOT, read, args, main, isMain } from "../lib/cli.mjs";
export const sorted = (object) =>
    Object.fromEntries(
        Object.keys(object)
            .sort()
            .map((k) => [k, object[k]]),
    );
if (isMain(import.meta.url))
    main(() => {
        args("Validate locale catalog formatting, placeholders and runtime archives.");
        const dir = path.join(ROOT, "crates/tsrs_diagnostics/loc"),
            generated = read(path.join(ROOT, "crates/tsrs_diagnostics/src/generated.rs"));
        const pairs = [
            ...generated.matchAll(/key: Key\(\s*("(?:[^"\\]|\\.)*")\s*,?\s*\),\s*text:\s*("(?:[^"\\]|\\.)*")/g),
        ];
        assert.equal(pairs.length, generated.split("key: Key(").length - 1, "could not parse all Rust diagnostics");
        const english = Object.fromEntries(pairs.map((m) => [JSON.parse(m[1]), JSON.parse(m[2])]));
        assert.equal(Object.keys(english).length, pairs.length);
        const names = [
            "cs-CZ",
            "de-DE",
            "es-ES",
            "fr-FR",
            "it-IT",
            "ja-JP",
            "ko-KR",
            "pl-PL",
            "pt-BR",
            "ru-RU",
            "tr-TR",
            "zh-CN",
            "zh-TW",
        ];
        assert.deepEqual(
            fs.readdirSync(dir).sort(),
            names.flatMap((n) => [n + ".generated.json", n + ".json.gz"]).sort(),
        );
        for (const name of names) {
            const handback = read(path.join(dir, `${name}.generated.json`)),
                messages = JSON.parse(handback);
            assert(messages && !Array.isArray(messages) && Object.values(messages).every((v) => typeof v === "string"));
            assert.equal(
                handback,
                JSON.stringify(sorted(messages), null, 2).replaceAll("\n", "\r\n"),
                `${name}: noncanonical handback`,
            );
            const placeholders = (text) => [...new Set(text.match(/\{\d+\}/g) ?? [])].sort();
            for (const [key, text] of Object.entries(messages))
                if (key in english)
                    assert.deepEqual(
                        placeholders(text),
                        placeholders(english[key]),
                        `${name}: placeholder mismatch for ${key}`,
                    );
            const archive = fs.readFileSync(path.join(dir, `${name}.json.gz`));
            assert.deepEqual(archive.subarray(0, 8), Buffer.from([31, 139, 8, 0, 0, 0, 0, 0]), `${name}: gzip header`);
            const payload = gunzipSync(archive).toString("utf8"),
                active = sorted(Object.fromEntries(Object.entries(messages).filter(([k]) => k in english)));
            assert.equal(payload, JSON.stringify(active), `${name}: runtime/handback mismatch`);
        }
        console.log(`validated ${names.length} locale pairs against ${pairs.length} Rust diagnostics`);
    });
