// The pinned async Node API client (the one rolldown-plugin-dts drives with `generator: "tsrs"`) runs
// `tsrs --api --runExternalCode` over testdata/contentmapper/types, with testdata/contentmapper/header-mapper installed
// as node_modules/header-mapper (as tools/contentmapper-e2e.sh does), and checks the diagnostics of the mapped file
// and of the plain file importing it against tsgo's output for the same project (expected.txt).
// Run: TS_REF=<ts-ref> TSRS=<tsrs> FIXTURES=<testdata/contentmapper> node --conditions=@typescript/source --test <this file>
import assert from "node:assert/strict";
import { cpSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

const api = `${process.env.TS_REF}/packages/typescript/src/api`;
const { API } = await import(`${api}/async/api.ts`);
const { formatDiagnostics } = await import(`${api}/diagnosticFormatter.ts`);

const fixtures = process.env.FIXTURES;
const dir = realpathSync(mkdtempSync(join(tmpdir(), "tsrs-api-contentmapper-")));
after(() => rmSync(dir, { recursive: true, force: true }));
cpSync(join(fixtures, "types"), dir, { recursive: true });
cpSync(join(fixtures, "header-mapper"), join(dir, "node_modules", "header-mapper"), { recursive: true });

test("diagnostics of a content-mapped file and its importer match tsgo", async () => {
    const client = new API({ tsserverPath: process.env.TSRS, cwd: dir, runExternalCode: true });
    try {
        const snapshot = await client.createSnapshot({ openProjects: [join(dir, "tsconfig.json")] });
        const project = snapshot.getConfiguredProject(join(dir, "tsconfig.json"));
        assert.ok(project, "the configured project is open");
        const diagnostics = [];
        for (const file of ["src/a.vue", "src/index.ts"]) {
            const fileName = join(dir, file);
            diagnostics.push(...await project.program.getSyntacticDiagnostics(fileName));
            diagnostics.push(...await project.program.getSemanticDiagnostics(fileName));
        }
        assert.equal(formatDiagnostics(diagnostics, client), readFileSync(join(dir, "expected.txt"), "utf8"));
    }
    finally {
        await client.close();
    }
});
