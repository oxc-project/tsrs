import type { A } from "__ENTRY0__";
import type { B } from "__ENTRY1__";
const a: A = {};
const b: B = {};
// @ts-expect-error first entry exports A only
import type { B as WrongFirst } from "__ENTRY0__";
// @ts-expect-error second entry exports B only
import type { A as WrongSecond } from "__ENTRY1__";
// @ts-expect-error unused dependency is absent from both entry APIs
import type { unused } from "__ENTRY0__";
// @ts-expect-error original A interface excludes null
const missing: A = null;
