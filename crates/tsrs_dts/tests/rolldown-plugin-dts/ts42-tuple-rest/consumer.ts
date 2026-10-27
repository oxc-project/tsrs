import type { UsesLeading, UsesMiddle } from "__ENTRY__";
const leading: UsesLeading = [{}, {}, 3];
const justNumber: UsesLeading = [3];
const middle: UsesMiddle = [true, {}, {}, false];
const noMiddle: UsesMiddle = [true, false];
// @ts-expect-error leading rest must terminate with number
const badLeading: UsesLeading = [{}, "end"];
// @ts-expect-error middle rest retains boolean endpoints
const badMiddle: UsesMiddle = [true, {}, 1];
