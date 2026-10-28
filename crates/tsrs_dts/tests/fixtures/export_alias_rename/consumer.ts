import type { Foo } from "__ENTRY__";
const good: Foo = { value: "text", extra: 1 };
// @ts-expect-error the aliased base interface is required
const missingBase: Foo = { extra: 1 };
// @ts-expect-error the derived interface's own member is required
const missingOwn: Foo = { value: "text" };
