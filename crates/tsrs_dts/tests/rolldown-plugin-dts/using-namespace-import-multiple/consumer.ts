import { Klass } from "__ENTRY__";
const instance: Klass = new Klass();
// @ts-expect-error abstract base does not make derived constructor accept arguments
new Klass(1);
// @ts-expect-error supplied empty heritage declares no methods
instance.missing();
// @ts-expect-error base stays private to the bundle
import { Base } from "__ENTRY__";
// @ts-expect-error implemented interface stays private to the bundle
import type { Iface } from "__ENTRY__";
