import { setAlignment } from "__ENTRY__";
import type { SeussFish } from "__ENTRY__";
const fish: SeussFish = "red fish";
const count: SeussFish = "two fish";
setAlignment("top-left");
setAlignment("middle-center");
// @ts-expect-error only declared quantity/color alternatives are allowed
const wrongFish: SeussFish = "green fish";
// @ts-expect-error horizontal and vertical positions retain ordering
setAlignment("left-top");
