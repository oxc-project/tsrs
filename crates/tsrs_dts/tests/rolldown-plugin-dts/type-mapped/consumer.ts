import type { Foo } from "__ENTRY__";
const emptyMapping: Foo = {};
type Keys = keyof Foo;
// @ts-expect-error source interface A has no keys, so mapping has none
const key: Keys = "unexpected";
// @ts-expect-error mapped object remains non-nullable
const nullable: Foo = null;
