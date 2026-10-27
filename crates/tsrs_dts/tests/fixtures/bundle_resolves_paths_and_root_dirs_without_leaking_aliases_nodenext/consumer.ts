import type { API } from '__ENTRY__';
const api: API = { options: { endpoint: 'url' }, generated: { version: 1 } };
// @ts-expect-error the generated type must retain its property type
const bad: API = { options: { endpoint: 'url' }, generated: { version: 'wrong' } };
