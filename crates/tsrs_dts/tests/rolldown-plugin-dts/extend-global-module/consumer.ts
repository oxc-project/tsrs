import value from '__ENTRY__';
declare const global: NodeJS.Global;
const first: {} = global.first;
const second: {} = global.second;
const second2: {} = global.second2;
// @ts-expect-error augmentation preserves the member set
global.missing;
// @ts-expect-error augmented support declarations are not entry exports
import type { Second } from '__ENTRY__';
