import type { API, Query } from '__ENTRY__';
const item: API.Item = { value: 'ok' };
declare const query: Query;
const version: number = query.Values.version;
// @ts-expect-error member must remain string
const bad: API.Item = { value: 1 };
