type UnionToIntersection<U> = (U extends U ? (value: U) => void : never) extends
  (value: infer Intersection) => void ? Intersection : never
