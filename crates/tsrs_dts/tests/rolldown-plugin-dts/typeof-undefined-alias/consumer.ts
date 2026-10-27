import { undefined as localUndefined } from "__ENTRY__";
import type { T } from "__ENTRY__";
const value: T = localUndefined;
const text: T = "shadowed";
// @ts-expect-error query refers to local string binding, not global undefined
const globalUndefined: T = undefined;
// @ts-expect-error local undefined binding remains string
const number: T = 1;
