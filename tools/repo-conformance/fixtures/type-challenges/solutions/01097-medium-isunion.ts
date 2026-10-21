type IsUnion<T, Whole = T> = [T] extends [never] ? false :
  T extends Whole ? [Whole] extends [T] ? false : true : never
