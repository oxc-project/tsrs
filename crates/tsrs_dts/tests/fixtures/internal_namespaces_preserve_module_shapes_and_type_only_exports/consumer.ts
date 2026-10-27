import { tools } from '__ENTRY__';
import type { API, Module, Inline } from '__ENTRY__';
const api: API = { value: 'ok', back: { item: { value: 'nested' } } };
const module: Module = tools;
const inline: Inline = tools;
const value: string = module.create().value;
const item: tools.Item = api;
// @ts-expect-error module shape must retain required value exports
const missing: Module = { version: 1 };
// @ts-expect-error type-only class must not become a value
new tools.Hidden();
// @ts-expect-error module query excludes type-only exports
module.Hidden;
// @ts-expect-error unexported values stay private
tools.secret;
// @ts-expect-error module support members are not entry exports
import { version } from '__ENTRY__';
