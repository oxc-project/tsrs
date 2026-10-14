export {};
class Café<T> {
  union(): undefined | Café<T> | Café<T> { return this; }
  alias(): Café<T> { const self = this; return self; }
  flow(flag: boolean): Café<T> {
    if (flag) { return this; }
    return new Café<T>();
  }
  explicit(this: Café<T>): Café<T> { return this; }
  nested(): Café<T> { function inner() { return new Café<T>(); } return this; }
  onlyNested(): Café<T> { const inner = () => this; throw new Error(); }
  parenthesized(): (Café<T>) { return this; }
  intersection(): Café<T> & {} { return this; }
  qualified(): globalThis.Café<T> { return this; }
  concise = (): Café<T> => this;
  expression = function (): Café<T> { return this; };
  wrapped = (() : Café<T> => this);
  static direct(): Café<T> { return this; }
  static indirect(): Café<T> { const self = this; return self; }
  get value(): Café<T> { return this; }
}
const object = { method(): Café<number> { return this; } };
const named = class 名 {
  direct(): 名 { return this; }
  alias(): 名 { const self = this; return self; }
  concise = (): 名 => this;
};
