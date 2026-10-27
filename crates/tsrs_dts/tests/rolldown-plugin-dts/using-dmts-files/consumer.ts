import { MyClass } from "__ENTRY__";
const instance: MyClass = new MyClass();
// @ts-expect-error declaration constructor has no parameters
new MyClass(1);
// @ts-expect-error no members are declared
instance.missing;
