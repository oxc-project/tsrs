import Default, { default2 } from '__ENTRY__';
const left: Default = new Default();
const right: default2 = new default2();
// @ts-expect-error retain the first class constructor's signature
new Default(1);
// @ts-expect-error retain the second renamed class constructor's signature
new default2(1);
// @ts-expect-error the original class names are not public entry exports
import { Foo } from '__ENTRY__';
