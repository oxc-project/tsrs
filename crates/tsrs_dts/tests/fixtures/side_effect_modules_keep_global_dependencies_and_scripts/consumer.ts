import type { API } from '__ENTRY__';
const item: API = { item: { id: 1, private: { value: 'ok' } } };
const external: ExternalGlobal = { ext: true };
// @ts-expect-error nested private dependency remains string
const bad: API = { item: { id: 1, private: { value: 1 } } };
