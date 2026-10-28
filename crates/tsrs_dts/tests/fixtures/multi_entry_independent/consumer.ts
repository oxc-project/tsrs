import { a } from '__ENTRY0__';
import { b } from '__ENTRY1__';
const number: number = a;
const string: string = b;
// @ts-expect-error entry export isolation
import { b as leaked } from '__ENTRY0__';
