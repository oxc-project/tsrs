import type { API } from '__ENTRY__';
const api: API = { item: { mode: 'require' } };
// @ts-expect-error the other conditional export must not be selected
const wrong: API = { item: { mode: 'import' } };
