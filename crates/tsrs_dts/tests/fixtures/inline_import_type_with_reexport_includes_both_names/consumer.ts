import type { Found, Keep } from "__ENTRY__";
const kept: Keep = { kept: true };
const found: Found = { found: true };
// @ts-expect-error resolving the inline import must not lose the named reexport
const wrongKeep: Keep = { kept: false };
// @ts-expect-error the inline import's target retains its required member
const missingFound: Found = {};
// @ts-expect-error the inline import's member retains its literal type
const wrongFound: Found = { found: false };
// @ts-expect-error resolving both names must not conflate their shapes
const wrongTarget: Keep = found;
// @ts-expect-error the inline import's target is not reexported by name
import type { Missing } from "__ENTRY__";
// @ts-expect-error unused dependency exports stay outside the public entry
import type { Unused } from "__ENTRY__";
