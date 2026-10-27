import type { Public } from "__ENTRY__";
const good: Public = { value: "text" };
// @ts-expect-error the original declaration's member type must survive the star barrel
const bad: Public = { value: 1 };
// @ts-expect-error the internal declaration name is not a public entry export
import type { Internal } from "__ENTRY__";
