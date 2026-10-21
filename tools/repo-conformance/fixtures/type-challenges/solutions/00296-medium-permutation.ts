type Permutation<T, Choice = T> = [T] extends [never] ? [] :
  Choice extends Choice ? [Choice, ...Permutation<Exclude<T, Choice>>] : never
