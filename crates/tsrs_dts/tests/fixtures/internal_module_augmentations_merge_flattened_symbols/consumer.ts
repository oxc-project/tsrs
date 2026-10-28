import type { Item } from '__ENTRY__';
const item: Item = { id: 1, extra: { value: 'ok' } };
// @ts-expect-error augmented member keeps its dependency type
const bad: Item = { id: 1, extra: { value: 1 } };
