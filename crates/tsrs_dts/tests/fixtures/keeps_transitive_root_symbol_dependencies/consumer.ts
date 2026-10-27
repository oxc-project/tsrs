import type { C } from "__ENTRY__";
const good: C = { a: 1 };
// @ts-expect-error the transitive alias must still constrain the member
const bad: C = { a: "wrong" };
// @ts-expect-error private support alias must not become a public export
import type { A } from "__ENTRY__";
