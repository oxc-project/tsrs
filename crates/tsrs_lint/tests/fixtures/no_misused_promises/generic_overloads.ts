
type EventName = 'a' | 'b' | 'c' | 'd' | 'e' | 'f' | 'g' | 'h' | 'i' | 'j' | 'k' | 'l' | 'm' | 'n' | 'o' | 'p';
type Hook = {
  [K in EventName]: (event: Record<K, 1> | Record<K, 2>) => void;
}[EventName];
interface On {
  (): void;
  (pattern: 'start', hook: () => void): void;
  <P extends string, M extends Hook>(pattern: P, matcher: M, hook: NoInfer<M>): void;
}
declare const on: On;
interface OptionalOn {
  (pattern: 'start', hook: () => void): void;
  <P extends string, M extends Hook>(pattern: P, matcher: M | undefined, hook: NoInfer<M>): void;
}
declare const optionalOn: OptionalOn;
interface AsyncOn {
  (pattern: 'start', hook: () => void): void;
  <P extends string, M extends Hook | (() => Promise<void>)>(pattern: P, matcher: M, hook?: NoInfer<M>): void;
}
declare const asyncOn: AsyncOn;
interface NewOn {
  new (pattern: 'start', hook: () => void): object;
  new <P extends string, M extends Hook>(pattern: P, matcher: M, hook: NoInfer<M>): object;
}
declare const newOn: NewOn;
