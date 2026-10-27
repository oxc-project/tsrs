import type { API } from '__ENTRY__';
const api: API = { item: { mode: 'import' } };
// @ts-expect-error the other conditional export must not be selected
const wrong: API = { item: { mode: 'require' } };
