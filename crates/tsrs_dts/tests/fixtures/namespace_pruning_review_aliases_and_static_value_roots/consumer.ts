import type { API, Make } from '__ENTRY__';
declare const make: Make;
const api: API = { ref: make() };
// @ts-expect-error static return type keeps nominal identity
const bad: API = { ref: { value: 1 } };
