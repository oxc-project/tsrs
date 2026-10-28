import type { SpreadedTuple } from "__ENTRY__";
const emptyRest: SpreadedTuple = [1];
const strings: SpreadedTuple = [1, "a", "b"];
// @ts-expect-error required first element
const missing: SpreadedTuple = [];
// @ts-expect-error rest elements remain strings
const badRest: SpreadedTuple = [1, 2];
