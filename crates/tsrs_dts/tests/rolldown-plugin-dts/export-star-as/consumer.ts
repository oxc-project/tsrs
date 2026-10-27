import { foo } from '__ENTRY__';
const a: foo.A = {};
// @ts-expect-error type-only namespace member has no runtime value
foo.A;
// @ts-expect-error namespace export does not flatten members
import type { A } from '__ENTRY__';
