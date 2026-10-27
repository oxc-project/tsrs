import type { API, Factory, Box, Inferred } from '__ENTRY__';
const api: API = { value: { id: 1, label: 'x' } };
const factory: Factory = value => ({ value });
const box: Box = { local: true };
const inferred: Inferred = { value: 1 };
// @ts-expect-error inferred import() return type must retain its property
const badInferred: Inferred = { value: 'wrong' };
// @ts-expect-error transitive inherited property remains required
const bad: API = { value: { label: 'x' } };
// @ts-expect-error type argument remains string
const wrong = factory(1);
// @ts-expect-error dependency must not become a public entry export
import type { Item } from '__ENTRY__';
