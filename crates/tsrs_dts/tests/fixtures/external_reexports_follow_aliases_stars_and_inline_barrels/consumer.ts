import { Client, create, count } from '__ENTRY__';
import type { API, Renamed, Inline, Module } from '__ENTRY__';
const item: Renamed = create(); const api: API = { item }; const inline: Inline = item;
const n: number = count;
declare const module: Module; const same: typeof Client = module.Client;
// @ts-expect-error preserve external nominal identity
const bad: Client = {};
// @ts-expect-error preserve member type
const wrong: Inline = { value: 1 };
