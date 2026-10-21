// Solutions and assertions for selected medium challenges from type-challenges/type-challenges.
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;
type Expect<T extends true> = T;

type Includes<T extends readonly unknown[], U> =
  T extends readonly [infer Head, ...infer Rest]
    ? Equal<Head, U> extends true ? true : Includes<Rest, U>
    : false;
type Flatten<T extends readonly unknown[]> =
  T extends readonly [infer Head, ...infer Rest]
    ? Head extends readonly unknown[] ? [...Flatten<Head>, ...Flatten<Rest>]
      : [Head, ...Flatten<Rest>]
    : [];
type AppendArgument<F extends (...args: any[]) => any, A> =
  F extends (...args: infer Args) => infer Result ? (...args: [...Args, A]) => Result : never;
type DeepReadonly<T> = T extends (...args: any[]) => any ? T :
  T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;
type Replace<S extends string, From extends string, To extends string> =
  From extends '' ? S : S extends `${infer Before}${From}${infer After}` ? `${Before}${To}${After}` : S;
type TrimLeft<S extends string> = S extends `${' ' | '\n' | '\t'}${infer Rest}` ? TrimLeft<Rest> : S;

type Cases = [
  Expect<Equal<Includes<['Kars', 'Esidisi'], 'Kars'>, true>>,
  Expect<Equal<Includes<[{ a: 1 }], { readonly a: 1 }>, false>>,
  Expect<Equal<Flatten<[1, [2, [3]], 4]>, [1, 2, 3, 4]>>,
  Expect<Equal<AppendArgument<(a: string) => number, boolean>, (a: string, x: boolean) => number>>,
  Expect<Equal<DeepReadonly<{ a: { b: number }; f: () => string }>,
    { readonly a: { readonly b: number }; readonly f: () => string }>>,
  Expect<Equal<Replace<'foo bar', 'bar', 'baz'>, 'foo baz'>>,
  Expect<Equal<Replace<'foo', '', 'x'>, 'foo'>>,
  Expect<Equal<TrimLeft<' \n\t hello'>, 'hello'>>
];

export type { Cases };
