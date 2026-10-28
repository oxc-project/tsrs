import type { LazyPerson, KindlessCircle } from "__ENTRY__";
const lazy: LazyPerson = { getName: () => "Ada", getAge: () => 30, getLocation: () => "London" };
const age: number = lazy.getAge();
const circle: KindlessCircle = { radius: 2 };
// @ts-expect-error getter keys are remapped with get and capitalization
lazy.name;
// @ts-expect-error getter return type is retained
const badAge: string = lazy.getAge();
// @ts-expect-error kind is removed by the conditional mapped key
const badCircle: KindlessCircle = { radius: 2, kind: "circle" };
