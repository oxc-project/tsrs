import { public_api_with_jsdoc, public_api_without_jsdoc } from '__ENTRY__';
import type { public_type } from '__ENTRY__';
const n: public_type = 1;
const a: void = public_api_with_jsdoc();
const b: void = public_api_without_jsdoc();
// @ts-expect-error the public type remains number
const wrong: public_type = 'text';
// @ts-expect-error unreferenced internal API must not leak through the entry
import { internal_api_with_jsdoc } from '__ENTRY__';
