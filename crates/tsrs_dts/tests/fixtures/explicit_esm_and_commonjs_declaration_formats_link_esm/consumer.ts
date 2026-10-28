import type { API, Client, Module } from '__ENTRY__';
const api: API = { value: 'ok' };
// @ts-expect-error private identity
const bad: Client = { value: 'wrong' };
