#!/usr/bin/env node
import { main } from "../tools/lib/cli.mjs";
import { cli } from "./measure.mjs";
main(() => cli(true));
