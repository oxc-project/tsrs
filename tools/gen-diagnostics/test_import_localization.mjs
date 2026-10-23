// Existing OneLoc importer regression cases, ported from unittest.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { importLocalization, readMessages } from "./import_localization.mjs";
import { write, writeJSON } from "../lib/cli.mjs";
const temporary = (fn) => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-localization-"));
    try {
        return fn(dir);
    } finally {
        fs.rmSync(dir, { recursive: true, force: true });
    }
};
test("preserves pending and filters obsolete translations", () =>
    temporary((dir) => {
        const source = path.join(dir, "source.json"),
            handback = path.join(dir, "handback.json"),
            out = path.join(dir, "loc/ko-KR.generated.json");
        writeJSON(source, {
            Updated: "Current English.",
            New: "New English.",
            StaleEnglish: "Changed English.",
            Translated: "Source [text].",
            SameAsEnglish: "English.",
            ExistingEnglish: "Existing English.",
            ExistingOnly: "Existing source.",
        });
        writeJSON(handback, {
            Updated: "Current English.",
            New: "New English.",
            StaleEnglish: "Old English.",
            Translated: "새 번역]",
            SameAsEnglish: "English.",
            Removed: "Obsolete translation.",
        });
        write(
            handback + ".lct",
            `<?xml version="1.0"?><LCX xmlns="http://schemas.microsoft.com/locstudio/2006/6/lcx"><Item ItemId=";String Table"><Item ItemId=";Strings"><Item ItemId=";Updated"><Str><Tgt Stat="Update"/></Str></Item><Item ItemId=";New"><Str><Tgt/></Str></Item><Item ItemId=";StaleEnglish"><Str><Tgt/></Str></Item><Item ItemId=";Translated"><Str><Tgt Stat="Loc"/></Str></Item><Item ItemId=";SameAsEnglish"><Str><Tgt Stat="Loc"/></Str></Item></Item></Item></LCX>`,
        );
        writeJSON(out, {
            Updated: "이전 번역",
            StaleEnglish: "기존 번역",
            Translated: "Previous translation.",
            SameAsEnglish: "Previous translation.",
            ExistingEnglish: "Existing English.",
            ExistingOnly: "Keep this translation.",
            Removed: "Remove this translation.",
        });
        const expected =
            '{\r\n  "ExistingOnly": "Keep this translation.",\r\n  "StaleEnglish": "기존 번역",\r\n  "Translated": "새 번역]",\r\n  "Updated": "이전 번역"\r\n}';
        for (let i = 0; i < 2; i++) {
            importLocalization(source, handback, out);
            assert.equal(fs.readFileSync(out, "utf8"), expected);
        }
        const fresh = path.join(dir, "new-locale/generated.json");
        importLocalization(source, handback, fresh);
        assert.deepEqual(readMessages(fresh), { Translated: "새 번역]" });
    }));
test("rejects bad metadata without changing output", () => {
    for (const [metadata, error] of [
        ["<LCX/>", /missing localization metadata/],
        ['<LCX><Item ItemId=";Key"><Str/></Item></LCX>', /missing localization metadata/],
        ["<LCX>", /malformed XML|LCX document/],
        ['<LCX><Item ItemId=";Key"><Str><Tgt Stat="Unexpected"/></Str></Item></LCX>', /unknown localization status/],
        ['<LCX><Item ItemId=";Key"><Str><Tgt Stat="New"/></Str></Item></LCX>', /unknown localization status/],
        ['<LCX><Item ItemId=";Key"><Str/></Item><Item ItemId=";Key"><Str/></Item></LCX>', /duplicate resource ID/],
        ['<LCX><Item ItemId="Key"><Str/></Item></LCX>', /invalid resource ID/],
    ])
        temporary((dir) => {
            const source = path.join(dir, "source.json"),
                handback = path.join(dir, "handback.json"),
                out = path.join(dir, "output.json");
            write(source, '\uFEFF{"Key":"English."}');
            writeJSON(handback, { Key: "Translation." });
            write(handback + ".lct", metadata);
            const existing = '{"Key":"Existing translation."}';
            write(out, existing);
            assert.throws(() => importLocalization(source, handback, out), error);
            assert.equal(fs.readFileSync(out, "utf8"), existing);
        });
});
