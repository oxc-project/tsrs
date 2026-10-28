import { a } from '__ENTRY0__';
import '__ENTRY1__';
const value: number = a;
// @ts-expect-error empty public entry exports no values
import { a as leaked } from '__ENTRY1__';
