import type { Resource } from '__ENTRY__';
const resource: Resource = { value: 'ok', [Symbol.dispose]() {} };
resource[Symbol.dispose]();
// @ts-expect-error Disposable requires a disposal method.
const missing: Resource = { value: 'missing' };
