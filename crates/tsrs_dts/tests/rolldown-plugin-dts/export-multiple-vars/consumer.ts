import { config, options, params } from '__ENTRY__';
import type { In, Out } from '__ENTRY__';
const input: In = { a: 'x' };
const a: Out = config.normalize(input);
const b: number = options.normalize(input).b;
const c: number = params.normalize(input).b;
// @ts-expect-error linked inline input type is not widened
config.normalize({ a: 1 });
// @ts-expect-error linked output field remains number
const wrong: string = a.b;
