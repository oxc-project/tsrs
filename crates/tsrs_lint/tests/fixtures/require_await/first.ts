const café = '🦀';
declare function work(): void;
declare function promise(): Promise<number>;
declare function decorate(...args: any[]): any;
async function outer() {
  async function nested() { await promise(); }
  work();
}
async function constrained<T extends Promise<number>>(value: T) { return value; }
const parenthesized = async () => ((promise()));
async function union(value: number | Promise<number>) { return value; }
async function callback(value: { then(cb: (n: number) => void): void }) { return value; }
async function noCallback(value: { then(cb: number): void }) { return value; }
async function* constrainedYield<T extends AsyncIterable<number>>(value: T) { yield* value; }
async function* asyncYield(value: Iterable<number> & AsyncIterable<number>) { yield* value; }
async function* syncYield(value: Iterable<number>) { yield* value; }
async function* bareYield() { yield; }
async function* literalYield() { yield 'é'; }
class Example {
  @decorate public async ['é'](): globalThis.Promise<number> { return 1; }
  field = 0
  async /* preserve 🦀 */ ['🦀']() { work(); }
  method() {}
  async [café]() { work(); }
}
work()
async /* é */ () => 1;
async function annotation(): Promise< /* é */ number > { return 1; }
async function* generator(): globalThis.AsyncGenerator<number> { yield 1; }
async function awaitUsing() { await using resource = null; }
async function empty() { /* é */ }
class TrailingComment {
  done() {} // ends é
  async ['é']() { work(); }
}
