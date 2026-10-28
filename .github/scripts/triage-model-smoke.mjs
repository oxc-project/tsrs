#!/usr/bin/env node
// Ad hoc probe of the exact provider implementation bundled with triagebot-action.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { appendFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const bundle = process.argv[2];
assert(bundle, "Usage: triage-model-smoke.mjs <pinned-action-dist/index.mjs> [--list]");
const temporary = mkdtempSync(join(tmpdir(), "tsrs-triage-models-"));
try {
    // Expose the bundled client without running the action's issue/PR handlers.
    const source = readFileSync(bundle, "utf8");
    const entrypoint = "main().catch((err2) => {\n  console.error(err2);\n  process.exitCode = 1;\n});";
    assert.equal(source.split(entrypoint).length, 2, "Pinned action entrypoint changed");
    const runtimePath = join(temporary, "runtime.mjs");
    writeFileSync(
        runtimePath,
        source.replace(entrypoint, "export { resolveModel, completeSimple, modelRegistry };"),
    );
    const { resolveModel, completeSimple, modelRegistry } = await import(pathToFileURL(runtimePath).href);
    const candidates = [
        "cloudflare-ai-gateway/claude-opus-4-6",
        "cloudflare-ai-gateway/claude-sonnet-4-6",
        ...[...modelRegistry.get("cloudflare-workers-ai").keys()].map((id) => `cloudflare-workers-ai/${id}`),
    ];
    if (process.argv.includes("--list")) {
        for (const candidate of candidates) {
            const model = resolveModel(candidate);
            console.log(`${candidate}\t${model.api}`);
        }
    } else {
        for (const name of ["CLOUDFLARE_API_KEY", "CLOUDFLARE_ACCOUNT_ID", "CLOUDFLARE_GATEWAY_ID"]) {
            assert(process.env[name], `${name} is required`);
        }
        const redact = (value) => {
            let text = String(value);
            for (const name of ["CLOUDFLARE_API_KEY", "CLOUDFLARE_ACCOUNT_ID"]) {
                text = text.replaceAll(process.env[name], "[redacted]");
            }
            return text.replaceAll(/\s+/g, " ").slice(0, 700);
        };
        const tool = {
            name: "smoke_check",
            description: "Returns a fresh verification token. Call once with value probe, then repeat its result exactly.",
            parameters: {
                type: "object",
                properties: { value: { type: "string", enum: ["probe"] } },
                required: ["value"],
                additionalProperties: false,
            },
        };
        async function probe(candidate) {
            const start = Date.now();
            let stage = "resolve";
            let toolCallPassed = false;
            const statuses = [];
            const complete = async (model, context) => {
                const result = await completeSimple(model, context, {
                    reasoning: "medium", // The triage runtime's default.
                    maxTokens: 2048,
                    signal: AbortSignal.timeout(60_000),
                    onResponse: (response) => statuses.push(response.status),
                });
                if (["error", "aborted", "length"].includes(result.stopReason)) {
                    throw new Error(result.errorMessage || `stopReason=${result.stopReason}`);
                }
                return result;
            };
            try {
                const model = resolveModel(candidate);
                const context = {
                    systemPrompt: "Follow the requested tool protocol exactly. After the tool returns, repeat only its result.",
                    messages: [{
                        role: "user",
                        content: 'Call smoke_check with {"value":"probe"}. Then repeat the token it returns exactly.',
                        timestamp: Date.now(),
                    }],
                    tools: [tool],
                };
                stage = "tool call";
                const first = await complete(model, context);
                const calls = first.content.filter((block) => block.type === "toolCall");
                assert.equal(first.stopReason, "toolUse", "Response did not finish with a tool call");
                assert.equal(calls.length, 1, "Expected exactly one tool call");
                const call = calls[0];
                assert.equal(call.name, tool.name, "Unexpected tool name");
                assert.deepEqual(call.arguments, { value: "probe" }, "Invalid tool arguments");
                toolCallPassed = true;
                stage = "tool result";
                const token = randomUUID();
                context.messages.push(first, {
                    role: "toolResult",
                    toolCallId: call.id,
                    toolName: call.name,
                    content: [{ type: "text", text: token }],
                    isError: false,
                    timestamp: Date.now(),
                });
                const second = await complete(model, context);
                const text = second.content.filter((block) => block.type === "text").map((block) => block.text).join("");
                assert.equal(second.stopReason, "stop", "Response did not finish normally");
                assert.equal(text.trim(), token, "Final response did not match the fresh tool result");
                return { candidate, ok: true, toolCallPassed, statuses, seconds: (Date.now() - start) / 1000 };
            } catch (error) {
                return { candidate, ok: false, toolCallPassed, statuses, stage, error: redact(error.message), seconds: (Date.now() - start) / 1000 };
            }
        }
        const results = [];
        // Limit concurrent requests while allowing slow models to time out independently.
        for (let offset = 0; offset < candidates.length; offset += 3) {
            const batch = await Promise.allSettled(candidates.slice(offset, offset + 3).map(probe));
            for (const result of batch) {
                if (result.status === "rejected") throw result.reason;
                results.push(result.value);
                console.log(JSON.stringify(result.value));
            }
        }
        const cell = (text) => String(text ?? "").replaceAll("|", "\\|").replaceAll("\n", " ");
        const summary = [
            "Model smoke test using the pinned triage runtime, medium reasoning, streaming, and a fresh tool result.",
            "",
            "| Model | Tool call | Tool result | Seconds | Failure |",
            "| --- | --- | --- | --- | --- |",
            ...results.map((r) => `| ${cell(r.candidate)} | ${r.toolCallPassed ? "PASS" : "FAIL"} | ${r.ok ? "PASS" : "FAIL"} | ${r.seconds.toFixed(1)} | ${cell(r.error)} |`),
            "",
        ].join("\n");
        console.log(summary);
        if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
        // Individual failures are diagnostic results; fail the job only if nothing works.
        if (!results.some((result) => result.ok)) process.exitCode = 1;
    }
} finally {
    rmSync(temporary, { recursive: true, force: true });
}
