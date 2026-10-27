import { Ns } from "__ENTRY__";
const reference: { value: string } = Ns.Ref;
const value: string = Ns.Ref.value;
// @ts-expect-error the namespace alias retains the referenced value's member type
const wrongValue: number = Ns.Ref.value;
// @ts-expect-error the namespace exposes the alias rather than the private name
Ns.Referenced;
// @ts-expect-error the namespace dependency must not become an entry export
import { Referenced } from "__ENTRY__";
