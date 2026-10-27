import type { Keep } from "__ENTRY__";
const good: Keep = { kept: true };
// @ts-expect-error the retained reexport keeps its literal member type
const wrongLiteral: Keep = { kept: false };
// @ts-expect-error the retained interface still requires its member
const missingMember: Keep = {};
// @ts-expect-error the dead inline-import alias is not an entry export
import type { Dead } from "__ENTRY__";
// @ts-expect-error the dead alias's dependency is not an entry export
import type { Unused } from "__ENTRY__";
