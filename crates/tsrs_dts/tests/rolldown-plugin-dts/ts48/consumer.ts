import type { SomeNum } from "__ENTRY__";
const hundred: SomeNum = 100;
// @ts-expect-error constrained template infer produces number literal 100
const other: SomeNum = 101;
// @ts-expect-error result is numeric, not the input string
const string: SomeNum = "100";
