import { NumberSchema } from "__ENTRY__";
class Derived extends NumberSchema { extra = "x"; }
const result: Derived = new Derived().min();
// @ts-expect-error typeof this does not lose the schema member
result.missing;
// @ts-expect-error min has no parameters
new NumberSchema().min(1);
