import type { API, Unicode } from '__ENTRY__'; const api: API = { value: 1 }; const unicode: Unicode = { label: 'ok' }; // @ts-expect-error
const bad: API = { value: 'wrong' }; // @ts-expect-error
const badUnicode: Unicode = { label: 1 };
