import type { Found, Keep } from "__ENTRY__";
const kept: Keep = { kept: true };
const found: Found = { found: true };
// @ts-expect-error the reexported interface retains its required member
const missingKeep: Keep = {};
// @ts-expect-error the inline import resolves to Missing's literal member
const wrongFound: Found = { found: false };
// @ts-expect-error the two interfaces remain distinct
const wrongTarget: Found = kept;
// @ts-expect-error the inline import's support interface is not an entry export
import type { Missing } from "__ENTRY__";
// @ts-expect-error unused dependency exports stay outside the public entry
import type { Unused } from "__ENTRY__";
