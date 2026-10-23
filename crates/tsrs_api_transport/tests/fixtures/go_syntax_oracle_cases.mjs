#!/usr/bin/env node
import { readFileSync } from "node:fs";
process.stdout.write(readFileSync(new URL("./go_syntax_oracle_cases.json", import.meta.url)));
