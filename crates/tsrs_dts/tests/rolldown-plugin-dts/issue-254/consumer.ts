import { Bar } from '__ENTRY__';
import type { Foo } from '__ENTRY__';
declare const foo: Foo;
const enumValue: number = foo.e;
declare const nestedEnum: Bar.F;
const nestedNumber: number = nestedEnum;
const enumObject: typeof Bar.F = Bar.F;
// @ts-expect-error the retained enum dependency is numeric
const wrong: string = foo.e;
// @ts-expect-error E is retained as a private dependency, not a public export
import { E } from '__ENTRY__';
