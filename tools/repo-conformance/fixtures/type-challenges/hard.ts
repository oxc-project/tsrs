// Solutions and assertions for selected hard challenges from type-challenges/type-challenges.
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;
type Expect<T extends true> = T;

type Permutation<T, U = T> = [T] extends [never] ? [] :
  U extends U ? [U, ...Permutation<Exclude<T, U>>] : never;
type UnionToIntersection<U> =
  (U extends U ? (arg: U) => void : never) extends (arg: infer I) => void ? I : never;
type IsUnion<T, C = T> = [T] extends [never] ? false :
  T extends C ? [C] extends [T] ? false : true : never;

type Cases = [
  Expect<Equal<Permutation<'A' | 'B'>, ['A', 'B'] | ['B', 'A']>>,
  Expect<Equal<Permutation<never>, []>>,
  Expect<Equal<UnionToIntersection<{ a: 1 } | { b: 2 }>, { a: 1 } & { b: 2 }>>,
  Expect<Equal<IsUnion<'a' | 'b'>, true>>,
  Expect<Equal<IsUnion<'a'>, false>>,
  Expect<Equal<IsUnion<never>, false>>
];

export type { Cases };
