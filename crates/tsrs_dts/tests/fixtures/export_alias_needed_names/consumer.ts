import type { MyType } from "__ENTRY__";
const good: MyType = { x: 1 };
// @ts-expect-error x must retain its original number type
const bad: MyType = { x: "wrong" };
// @ts-expect-error the supporting module's unused type must not become public
import type { Unused } from "__ENTRY__";
