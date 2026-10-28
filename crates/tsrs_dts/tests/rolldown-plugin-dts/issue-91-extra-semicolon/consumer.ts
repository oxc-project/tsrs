import type { Connection } from '__ENTRY__';
const connection: Connection = { uri: 'https://example.test' };
const uri: string = connection.uri;
// @ts-expect-error the property remains required
const missing: Connection = {};
// @ts-expect-error the property retains its string type
const wrong: Connection = { uri: 1 };
