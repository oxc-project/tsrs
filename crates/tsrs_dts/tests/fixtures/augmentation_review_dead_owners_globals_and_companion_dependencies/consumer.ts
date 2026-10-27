import type { Item, GlobalAlias, API } from '__ENTRY__';
const item: Item = { id: 1 };
const global: GlobalAlias = { global: true };
const api: API = { virtual: { local: item } };
// @ts-expect-error imported dependency keeps its property type
const bad: API = { virtual: { local: { id: 'wrong' } } };
