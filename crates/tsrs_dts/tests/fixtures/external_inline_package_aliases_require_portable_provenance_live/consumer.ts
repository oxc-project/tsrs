import type { API } from '__ENTRY__'; const api: API = { value: 'ok' }; // @ts-expect-error
const bad: API = { value: 1 };
