import type { Thing } from "__ENTRY__";
declare let thing: Thing;
thing.size = true;
thing.size = {};
const read: {} = thing.size;
// @ts-expect-error setter union still excludes null
thing.size = null;
// @ts-expect-error getter is an empty interface rather than boolean
const booleanRead: boolean = thing.size;
