import { create, Client } from '__ENTRY__';
import type { ClientOptions, ServerOptions } from '__ENTRY__';
const c: ClientOptions = { endpoint: 'url' };
const s: ServerOptions = { port: 80 };
const connected: boolean = create(c).connected;
// @ts-expect-error source modules' Options must remain distinct
const wrong: ClientOptions = s;
// @ts-expect-error supporting private types must not become package exports
import type { State } from '__ENTRY__';
// @ts-expect-error exported type must not become an instantiable value
const made = new Client();
// @ts-expect-error private class brand must remain nominal
const forged: Client = { value: 1 };
