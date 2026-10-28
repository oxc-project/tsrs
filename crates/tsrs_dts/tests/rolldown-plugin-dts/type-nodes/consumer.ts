import { parenthesized, union, intersection, operator, arrayAndTuple, predicate, assertion } from "__ENTRY__";
const first: {} = parenthesized({});
const either: {} = union({});
const both: {} = intersection({});
const list: {}[] = arrayAndTuple([{}, {}]);
declare let unknownValue: unknown;
if (predicate(unknownValue)) { const narrowed: {} = unknownValue; }
assertion(unknownValue);
const asserted: {} = unknownValue;
// @ts-expect-error operator parameter is keyof empty interface, namely never
operator("anything");
// @ts-expect-error tuple retains two required elements
arrayAndTuple([{}]);
// @ts-expect-error predicate result remains boolean
const wrongPredicate: string = predicate({});
// @ts-expect-error assertion call narrows unknown to non-nullable interface
const nullAfterAssertion: null = unknownValue;
