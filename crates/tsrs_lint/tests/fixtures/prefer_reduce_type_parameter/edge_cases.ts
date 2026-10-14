export {};
type 雪 = number[];
declare const 配列: number[];
配列.reduce((a, b) => a, [] /* 保持 */ as /* 型 */ 雪);
配列.reduce((a, b) => a, < /* 型 */ 雪> /* 保持 */ []);
配列.reduce<雪>((a, b) => a, [] as 雪);
配列.reduce?.((a, b) => a, [] as 雪);
const method = 'reduce' as const;
配列[method]((a, b) => a, [] as 雪);
配列[`reduce`]((a, b) => a, [] as 雪);
declare const methodUnion: 'reduce' | 'map';
配列[methodUnion]((a, b) => a, [] as 雪);
(配列.reduce)((a, b) => a, [] as 雪);
配列.reduce((a, b) => a, ([] as 雪));
配列.reduce((a, b) => a, {} as { required: string });
配列.reduce((a, b) => a, { extra: true } as { optional?: string });
declare const immutable: readonly number[];
immutable.reduce((a, b) => a, [] as 雪);
declare const maybe: number[] | null;
maybe?.reduce((a, b) => a, [] as 雪);
declare const mixed: number[] | { reduce: Function };
mixed.reduce((a, b) => a, [] as 雪);
declare const loose: any;
loose.reduce((a, b) => a, [] as 雪);
declare const impossible: never;
impossible.reduce((a, b) => a, [] as 雪);
function chained<T extends readonly number[], U extends T>(items: U) {
  return items.reduce((a, b) => a, [] as 雪);
}
function unconstrained<T>(items: T) {
  return items.reduce((a, b) => a, [] as 雪);
}
function asserted<T extends object>() {
  return 配列.reduce((a, b) => a, {} as T);
}
配列['reduce']((a, b) => a, [] as const);
配列.reduce((a, b) => a, <雪>([] as 雪));
