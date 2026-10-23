#!/usr/bin/env node
// Hex-encoded wire inputs retain invalid UTF-8 and duplicate JSON keys exactly.
import { readFileSync } from "node:fs";
process.stdout.write(readFileSync(new URL("./go_json_oracle_cases.json", import.meta.url)));
