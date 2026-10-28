import type { API } from '__ENTRY__';
import type { Item } from 'virtual';
const item: Item = { id: 1, value: 'ok' };
const api: API = { x: item };
// @ts-expect-error ambient declaration keeps property types
const bad: Item = { id: 1, value: 1 };
