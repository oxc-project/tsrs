import type { UsesFooBar } from "__ENTRY__";
const good: UsesFooBar = 1;
// @ts-expect-error the retained alias is numeric
const bad: UsesFooBar = "wrong";
// @ts-expect-error an unrelated private alias must not become public
import type { Foo } from "__ENTRY__";
