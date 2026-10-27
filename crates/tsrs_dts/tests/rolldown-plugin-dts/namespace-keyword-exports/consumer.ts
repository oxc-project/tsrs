import { foo } from '__ENTRY__';
const dot: 'foo' = foo.in;
const bracket: 'foo' = foo['in'];
// @ts-expect-error retain the literal value behind the keyword export
const wrong: 'bar' = foo.in;
// @ts-expect-error the namespace exposes only the exported keyword alias
foo._in;
