export function f({ a, b = 1 } = {}) {
  return [a, b];
}
f({ a: 1 });
