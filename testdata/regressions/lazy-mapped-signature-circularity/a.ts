const result = make() ? 1 : 2;
declare function make(): Value;
interface Value { then: M }
type M = { [K in "x"]: typeof result };
export {};
