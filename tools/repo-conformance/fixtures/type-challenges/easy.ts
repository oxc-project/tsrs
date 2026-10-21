// Solutions to the easy challenges at https://github.com/type-challenges/type-challenges.
// Keep the assertions here: a template with `any` would compile without exercising the answers.
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;
type Expect<T extends true> = T;

type MyPick<T, K extends keyof T> = { [P in K]: T[P] };
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
type TupleToObject<T extends readonly PropertyKey[]> = { [P in T[number]]: P };
type First<T extends readonly unknown[]> = T extends readonly [infer Head, ...unknown[]] ? Head : never;
type Length<T extends readonly unknown[]> = T['length'];
type MyExclude<T, U> = T extends U ? never : T;
type MyAwaited<T extends PromiseLike<unknown>> = T extends PromiseLike<infer U>
  ? U extends PromiseLike<unknown> ? MyAwaited<U> : U : never;
type If<C extends boolean, T, F> = C extends true ? T : F;
type Concat<T extends readonly unknown[], U extends readonly unknown[]> = [...T, ...U];

type Todo = { title: string; description: string; completed: boolean };
type Cases = [
  Expect<Equal<MyPick<Todo, 'title' | 'completed'>, { title: string; completed: boolean }>>,
  Expect<Equal<MyReadonly<Todo>, { readonly title: string; readonly description: string; readonly completed: boolean }>>,
  Expect<Equal<TupleToObject<['tesla', 'model 3']>, { tesla: 'tesla'; 'model 3': 'model 3' }>>,
  Expect<Equal<First<[3, 2, 1]>, 3>>,
  Expect<Equal<First<[]>, never>>,
  Expect<Equal<Length<readonly ['a', 'b']>, 2>>,
  Expect<Equal<MyExclude<'a' | 'b' | 'c', 'a' | 'c'>, 'b'>>,
  Expect<Equal<MyAwaited<Promise<Promise<string>>>, string>>,
  Expect<Equal<If<true, 'yes', 'no'>, 'yes'>>,
  Expect<Equal<If<false, 'yes', 'no'>, 'no'>>,
  Expect<Equal<Concat<[1, 2], ['a']>, [1, 2, 'a']>>
];

export type { Cases };
