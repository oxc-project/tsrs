import type { API, Factory, Module } from '__ENTRY__';
const api: API = { value: 'x' };
const factory: Factory = () => api;
const module: Module = { create: factory };
// @ts-expect-error external member type must remain string
const bad: API = { value: 1 };
