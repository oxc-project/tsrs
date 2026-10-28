import { ns1, ns2 } from '__ENTRY__';
const shape: ns1.A = {};
const value: string = ns1.E;
const alias: ns2.F = 'text';
const called: void = ns1.B();
const instance: ns2.C = new ns2.C();
const enumValue: 0 = ns1.D.A;
const genericClass: ns1.GenericC<string, number> = new ns2.GenericC<string, number>();
const genericCall: void = ns2.GenericF<string, number>();
const genericInterface: ns1.GenericI<string, number> = {};
const genericAlias: ns2.GenericT<string, number> = genericInterface;
const sameNamespace: typeof ns1 = ns2;
// @ts-expect-error retain namespace value annotations
const wrong: number = ns2.E;
// @ts-expect-error function retains its generic arity
ns1.GenericF<string>();
// @ts-expect-error type-only namespace exports cannot become runtime members
ns1.F;
