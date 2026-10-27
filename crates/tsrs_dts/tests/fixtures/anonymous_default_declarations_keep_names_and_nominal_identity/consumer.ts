import { Client, create } from '__ENTRY__';
const instance = new Client(); const value: string = create(instance.value).value;
// @ts-expect-error private identity
const bad: Client = { value: 'wrong', data: { marker: true } };
// @ts-expect-error argument remains string
create(1);
