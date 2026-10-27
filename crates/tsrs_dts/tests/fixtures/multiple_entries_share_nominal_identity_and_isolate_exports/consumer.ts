import { Client } from '__ENTRY0__';
import { Renamed } from '__ENTRY1__';
const a: Client = new Renamed();
const b: Renamed = new Client();
// @ts-expect-error each entry retains only its own exports
import type { Other } from '__ENTRY0__';
// @ts-expect-error private nominal brand remains shared
const forged: Client = { value: 1 };
