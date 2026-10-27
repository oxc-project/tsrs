import { B } from '__ENTRY__';
const b: B = new B();
// @ts-expect-error star cycle does not add unrequested entry export
import { A } from '__ENTRY__';
