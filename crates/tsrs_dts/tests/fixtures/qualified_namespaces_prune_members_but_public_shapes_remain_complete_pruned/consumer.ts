import type { API, Factory } from '__ENTRY__';
const api: API = { id: 1, value: 'ok' };
const make: Factory = () => api;
// @ts-expect-error inherited dependency remains required
const bad: API = { value: 'ok' };
