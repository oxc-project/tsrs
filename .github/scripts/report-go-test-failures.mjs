#!/usr/bin/env node
// Replay Go JSON output for failed or unfinished groups, including package-level crash diagnostics.
import fs from "node:fs";

const events = [];
for (const line of fs.readFileSync(process.argv[2], "utf8").split("\n")) {
    if (!line.trim()) continue;
    try {
        events.push(JSON.parse(line));
    } catch {
        // A killed process can leave its final event incomplete; retain it with the other diagnostics.
        console.error(line);
    }
}

const outcomes = new Map();
for (const event of events) {
    if (event.Test && !event.Test.includes("/") && ["pass", "fail", "skip"].includes(event.Action)) {
        outcomes.set(event.Test, event.Action);
    }
}
for (const event of events) {
    if (!event.Output) continue;
    const outcome = outcomes.get(event.Test?.split("/", 1)[0]);
    if (!event.Test || (outcome !== "pass" && outcome !== "skip")) process.stdout.write(event.Output);
}
