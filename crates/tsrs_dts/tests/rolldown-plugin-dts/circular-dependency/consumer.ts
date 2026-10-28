import { A } from '__ENTRY__';
const a: A = new A();
// @ts-expect-error unused cyclic export stays private to entry
import { B } from '__ENTRY__';
