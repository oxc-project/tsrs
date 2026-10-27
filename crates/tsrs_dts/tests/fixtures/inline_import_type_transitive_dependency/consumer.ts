import type { T } from "__ENTRY__";
const good: T = { base: "text", child: true };
// @ts-expect-error the inline-imported interface retains its inherited member
const missingBase: T = { child: true };
// @ts-expect-error the inherited member retains its string type
const wrongBase: T = { base: 1, child: true };
// @ts-expect-error the inline-imported interface retains its own member
const missingChild: T = { base: "text" };
// @ts-expect-error the child's member retains its literal type
const wrongChild: T = { base: "text", child: false };
// @ts-expect-error the transitive base is not an entry export
import type { Base } from "__ENTRY__";
// @ts-expect-error the inline import's target is not an entry export
import type { Child } from "__ENTRY__";
// @ts-expect-error unused dependency exports stay outside the public entry
import type { Unused } from "__ENTRY__";
