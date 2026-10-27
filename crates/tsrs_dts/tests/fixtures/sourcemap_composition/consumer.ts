// This contract checks declaration code only, not source-map composition.
import type { Foo } from "__ENTRY__";
const good: Foo = { bar: 1 };
// @ts-expect-error typeof foo must remain number
const bad: Foo = { bar: "wrong" };
