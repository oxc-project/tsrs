import type { Module } from '__ENTRY__'; const module: Module = { value: 1 }; // @ts-expect-error
const bad: Module = { value: 'wrong' };
