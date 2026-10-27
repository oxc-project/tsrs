import { GenericKlass, genericFunction } from "__ENTRY__";
import type { GenericInterface, ConditionalInfer, Mapped, GenericType, GenericExtends } from "__ENTRY__";
const klass = new GenericKlass<string>();
const defaults: string = klass.b;
const iface: GenericInterface<number> = { c: 1, d: 2 };
const result: string = genericFunction<string>("x");
const inferred: ConditionalInfer<number[][]> = 1;
const mapped: Mapped<{ field: string }> = { field: "x" };
const generic: GenericType<boolean> = { k: true, l: false };
const inherited: GenericExtends<string> = { c: "x", d: "y" };
// @ts-expect-error B defaults to type parameter A, not the unrelated interface A
klass.b = 1;
// @ts-expect-error generic default D follows C
const wrong: GenericInterface<number> = { c: 1, d: "x" };
// @ts-expect-error infer H remains the nested array element
const wrongInfer: ConditionalInfer<number[][]> = "x";
// @ts-expect-error mapped value type follows the input property
mapped.field = 1;
// @ts-expect-error unrelated exported interface from dependency is pruned from entry
import type { A } from "__ENTRY__";
