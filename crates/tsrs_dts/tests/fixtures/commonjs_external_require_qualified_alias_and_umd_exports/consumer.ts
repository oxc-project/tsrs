import { Alias } from '__ENTRY__'; const client = new Alias(); const value: string = client.value;
// @ts-expect-error nominal identity
const bad: Alias = { value: 'wrong' };
