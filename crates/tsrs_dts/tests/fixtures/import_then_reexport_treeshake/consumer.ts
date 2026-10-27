import type { Needed } from "__ENTRY__";
const good: Needed = { x: 1 };
// @ts-expect-error retained member must remain numeric
const bad: Needed = { x: false };
// @ts-expect-error unused sibling export must not leak through the entry
import type { Unneeded } from "__ENTRY__";
