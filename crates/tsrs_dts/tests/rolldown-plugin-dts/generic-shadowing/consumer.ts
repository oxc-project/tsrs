import type { Config1, Config2, Client } from '__ENTRY__';
const first: Config1<number> = 1;
const second: Config2<string> = 'x';
const client: Client = null;
// @ts-expect-error generic parameter does not resolve to outer any
const wrongFirst: Config1<number> = 'x';
// @ts-expect-error independently scoped parameter remains generic
const wrongSecond: Config2<string> = 1;
