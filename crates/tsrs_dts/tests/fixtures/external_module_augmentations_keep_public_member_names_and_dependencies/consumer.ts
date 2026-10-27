import type { Item, Private } from '__ENTRY__';
const item: Item = { id: 1, extra: { value: 'ok' } };
const own: Private = { own: true };
// @ts-expect-error augmented member keeps its dependency type
const bad: Item = { id: 1, extra: { value: 1 } };
