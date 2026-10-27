// Checks the synthetic explicit-config build only; nearest-config discovery is unsupported.
import type { A } from '__ENTRY0__';
import type { B } from '__ENTRY1__';
const a: A = {};
const b: B = {};
// @ts-expect-error each entry retains its own export set
import type { B as WrongB } from '__ENTRY0__';
// @ts-expect-error the shared but unused interface is not public
import type { I } from '__ENTRY1__';
