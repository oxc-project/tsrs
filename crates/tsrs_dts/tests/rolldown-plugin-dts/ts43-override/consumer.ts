import { SpecializedComponent } from "__ENTRY__";
const component = new SpecializedComponent();
const shown: {} = component.show();
const hidden: {} = component.hide();
// @ts-expect-error override retains zero-argument signature
component.show(1);
// @ts-expect-error hide returns an interface rather than a number
const bad: number = component.hide();
// @ts-expect-error base class is supporting, not public at entry
import { SomeComponent } from "__ENTRY__";
