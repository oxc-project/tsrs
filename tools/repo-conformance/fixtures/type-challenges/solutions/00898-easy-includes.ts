type IncludesEqual<X, Y> = (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false

type Includes<T extends readonly unknown[], U> = T extends readonly [infer Head, ...infer Tail]
  ? IncludesEqual<Head, U> extends true ? true : Includes<Tail, U>
  : false
