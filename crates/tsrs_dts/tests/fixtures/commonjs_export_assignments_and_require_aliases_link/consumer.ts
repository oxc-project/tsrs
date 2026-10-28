import Client = require('__ENTRY__');
const instance = new Client(); const value: string = instance.value;
const opts: Client.Options = { name: 'ok' };
// @ts-expect-error nominal identity
const bad: Client = { value: 'wrong' };
