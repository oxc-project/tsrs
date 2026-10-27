import { Client } from '__ENTRY__';
import type { Space, Item } from '__ENTRY__';
const client = new Client();
const options: Client.Options = { name: 'ok' };
const box: Space.Box = { item: { value: 'ok' } };
const nested: Space.Nested.Value = box.item;
const item: Item = { other: true };
// @ts-expect-error private class identity must survive merging
const forged: Client = { value: 1 };
// @ts-expect-error nested member keeps its dependency type
const bad: Space.Box = { item: { value: 1 } };
