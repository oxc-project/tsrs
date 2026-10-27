import type { A } from '__ENTRY__';
declare const a: A;
const globalA: Named.Core.A = a;
const globalB: Foo.Bar.Baz.Quux.B = {};
// @ts-expect-error preserve the global namespace member set
type Missing = Named.Core.Missing;
