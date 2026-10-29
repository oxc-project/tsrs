// `Pass<never, U>` is a deferred conditional type whose raw true type is a mapped type; instantiated with T = never
// that mapped type is `never`, so the conditional has the `T extends U ? never : T` shape and simplifies to `any`.
// The simplification runs when the declared type is normalized for the assignment below. A fast path that treats
// every mapped true type as an object type (one that cannot be `never`) skips it and reports TS2322 here.
type Pass<T, U> = any extends U ? { [K in keyof T]: T[K] } : any;

function f<U>() {
  const x: Pass<never, U> = "str";
  return x;
}

// The same shape through an array-valued type variable: `U[]` is an object type, so the simplification does not
// apply and the deferred conditional is related branch by branch.
type Wrap<T, U> = U[] extends U ? { [K in keyof T]: T[K] } : U[];

function g<U>() {
  const y: Wrap<never, U> = "str";
  return y;
}
