import { Client } from '__ENTRY__';
import type { API } from '__ENTRY__';
const value: Client = { value: 1 };
const api: API = { client: value };
// @ts-expect-error normal re-exports must not turn the original type-only class into a value
const runtime = new Client();
