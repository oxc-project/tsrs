import type { A } from '__ENTRY0__';
import type { B } from '__ENTRY1__';
const a: A = {};
const b: B = {};
// @ts-expect-error B is exported only by the second entry
import type { B as WrongB } from '__ENTRY0__';
// @ts-expect-error A is exported only by the first entry
import type { A as WrongA } from '__ENTRY1__';
// @ts-expect-error unused common declarations must not become public
import type { unused } from '__ENTRY0__';
