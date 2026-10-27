import type { FirstHammer, State } from "__ENTRY__";
const first: FirstHammer<[{ weight: 1 }, string]> = { weight: 1 };
const state: State<string> = { get: () => "x", set: value => { const text: string = value; } };
// @ts-expect-error inferred H retains first element structure
const wrongFirst: FirstHammer<[{ weight: 1 }, string]> = {};
// @ts-expect-error an empty tuple has no first element
const noFirst: FirstHammer<[]> = {};
// @ts-expect-error in out annotation keeps State invariant
const wider: State<string | number> = state;
