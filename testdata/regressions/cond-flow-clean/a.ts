// Type nodes whose resolved type depends on the conditional-flow substitution (a true branch, an unnamed
// mapped-type template or a parameter between the node and its statement) next to the same nodes in clean
// positions. A substitution skipped or applied wrongly flips a TS2344 constraint error or a probed type.
type Num = number | `${number}`;
type NumKey<N extends Num> = N;
type Up<S extends string> = Uppercase<S>;

// Clean position: no substitution, the constraint error is expected.
type T0<T> = Up<T>;

// True branch of a conditional type: T is substituted with T & string.
type T1<T> = T extends string ? Up<T> : never;

// False branch: no substitution.
type T2<T> = T extends string ? never : Up<T>;

// Nested true branches: both substitutions apply; the inner false branch still sees the outer one.
type T3<T> = T extends string ? (T extends `a${string}` ? Up<T> : Lowercase<T>) : never;

// A parameter annotation in a true branch (contravariant position; type variables are still substituted).
type T4<T> = T extends string ? (x: Up<T>) => void : never;

// A method parameter and a property two levels deep in a type literal in a true branch.
type T5<T> = T extends string ? { m(x: Up<T>): void; p: Up<T> } : never;

// A callback parameter under a parameter: variance flips twice.
type T6<T> = T extends string ? (cb: (y: Up<T>) => void) => void : never;

// Homomorphic mapped type over a tuple-constrained variable: K gains the numeric constraint.
type T7<T extends readonly unknown[]> = { [K in keyof T]: NumKey<K> };

// A named mapped type (with `as`) adds no constraint.
type T8<T extends readonly unknown[]> = { [K in keyof T as K]: NumKey<K> };

// A mapped-type template nested in a conditional true branch.
type T9<T> = T extends readonly unknown[] ? { [K in keyof T]: NumKey<K> } : never;

// Unary tuple check and extends types substitute through the tuple.
type T10<T> = [T] extends [string] ? Up<T> : never;

// A parameter annotation at statement level is a clean position.
declare function f0<T>(x: Up<T>): void;
declare function f1<T extends string>(x: Up<T>): void;

// A statement inside a parameter initializer: the statement boundary ends the walk.
function h<T>(a = (): void => { let v: Up<T>; void v; }) { void a; }

// Probed results.
const c1: T1<"ab"> = 1;
const c3: T3<"ab" | "xy"> = 1;
const c4: T4<"ab"> = 1;
const c7: T7<[string, number]> = 1;
const c9: T9<["a"]> = 1;
const c10: T10<"ab"> = 1;
void [c1, c3, c4, c7, c9, c10];
