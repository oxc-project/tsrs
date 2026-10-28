import type { Constructor, Default } from '__ENTRY__';
declare const ctor: Constructor;
const instance = new ctor();
const value: number = instance.value;
const defaultValue: Default = { value: 'ok' };
// @ts-expect-error private class identity must remain nominal
const forged: InstanceType<Constructor> = { value: 1 };
// @ts-expect-error support constructor must not become a public value
import { Client } from '__ENTRY__';
