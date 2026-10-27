import type { Foo } from "__ENTRY__";
const branch: Foo = {};
// @ts-expect-error matching empty interfaces select C rather than null
const wrongBranch: Foo = null;
// @ts-expect-error dependency interfaces are not public exports
import type { A } from "__ENTRY__";
