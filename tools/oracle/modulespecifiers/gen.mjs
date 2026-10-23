#!/usr/bin/env node
// Module specifier cases are data, shared by both compiler oracles.
import { readFileSync } from "node:fs";
process.stdout.write(readFileSync(new URL("./scenarios.jsonl", import.meta.url)));
