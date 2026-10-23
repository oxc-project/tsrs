#!/usr/bin/env node
import path from "node:path";
import { ROOT, args, sleep, main } from "../../lib/cli.mjs";
import { Server, TSRS_WORK } from "../../lib/lsp.mjs";
async function check(command, shutdown, answer) {
    const server = new Server("exit-check", command, ROOT, undefined, process.env, answer);
    server.send({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { processId: null, rootUri: null, capabilities: {} },
    });
    server.notify("initialized", {});
    await sleep(500);
    if (shutdown) server.send({ jsonrpc: "2.0", id: 2, method: "shutdown" });
    server.notify("exit", undefined);
    let timer;
    try {
        const code = await Promise.race([
            server.done,
            new Promise((resolve) => {
                timer = setTimeout(() => {
                    server.proc.kill("SIGKILL");
                    resolve("TIMEOUT");
                }, 10000);
            }),
        ]);
        await server.done;
        return [code, server.error.trim().slice(-60)];
    } finally {
        clearTimeout(timer);
    }
}
main(async () => {
    const a = args("Compare language-server exit behavior.", {
        a: path.join(TSRS_WORK, "bin/tsgo-ref") + " --lsp -stdio",
        b: path.join(ROOT, "target/release/tsrs") + " --lsp -stdio",
    });
    let bad = 0;
    for (const answer of [true, false])
        for (const shutdown of [true, false]) {
            const before = await check(a.a, shutdown, answer),
                after = await check(a.b, shutdown, answer),
                equal = JSON.stringify(before) === JSON.stringify(after);
            if (!equal) bad++;
            console.log(
                `${shutdown ? "shutdown" : "no shutdown"} ${answer ? "answers" : "silent client"} tsgo-ref=${JSON.stringify(before)} tsrs=${JSON.stringify(after)} ${equal ? "equal" : "DIFFERENT"}`,
            );
        }
    process.exitCode = bad ? 1 : 0;
});
