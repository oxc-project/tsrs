type D<T> = { x: T extends string ? 1 : T };
function g<U, V, W>(b: D<U | V>, c: D<W>, k: boolean) {
    const n: number = k ? b.x : c.x;
    const m: number = k ? c.x : b.x;
}
