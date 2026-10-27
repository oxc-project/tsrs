import { Renamed } from '__ENTRY__';
import type { Module } from '__ENTRY__';
const item: Renamed.Client = { value: 1 };
// @ts-expect-error type-only namespace cannot provide a constructor
new Renamed.Client();
declare const module: Module;
declare const key: keyof Module;
const noKey: never = key;
// @ts-expect-error type-only namespace is absent from the module value
module.Renamed;
