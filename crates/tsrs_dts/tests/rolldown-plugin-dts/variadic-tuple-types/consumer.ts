import { concat } from "__ENTRY__";
import type { StrStrNumNumBool } from "__ENTRY__";
const tuple: StrStrNumNumBool = ["a", "b", 1, 2, true];
const concatenated: ["a", 1, true] = concat(["a"] as const, [1, true] as const);
// @ts-expect-error spread tuple retains final boolean
const wrongEnd: StrStrNumNumBool = ["a", "b", 1, 2, "true"];
// @ts-expect-error concatenation retains literal order
const wrongOrder: [1, "a", true] = concat(["a"] as const, [1, true] as const);
