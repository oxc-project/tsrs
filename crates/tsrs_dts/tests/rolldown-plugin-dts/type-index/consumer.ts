import type { Foo } from "__ENTRY__";
const dictionary: Foo = { anyKey: {}, another: { nested: true } };
const value: {} = dictionary["anyKey"];
// @ts-expect-error indexed interface values exclude null
const bad: Foo = { anyKey: null };
// @ts-expect-error interface dependency remains private
import type { A } from "__ENTRY__";
