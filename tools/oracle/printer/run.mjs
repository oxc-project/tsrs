#!/usr/bin/env node
import { main } from "../../lib/cli.mjs";
import { driver } from "../hash.mjs";
main(() => driver(true));
