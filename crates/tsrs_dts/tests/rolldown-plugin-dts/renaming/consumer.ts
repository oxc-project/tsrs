import { AKlass, BKlass, AFunc, BFunc } from "__ENTRY__";
import type { AInterface, BInterface, AType, BType } from "__ENTRY__";
const a: AKlass = new AKlass();
const b: BKlass = new BKlass();
const aInterface: AInterface = { c: a.a };
const bInterface: BInterface = { c: b.a };
const aType: AType = { f: AFunc({}) };
const bType: BType = { f: BFunc({}) };
// @ts-expect-error renamed function retains required argument
AFunc();
// @ts-expect-error renamed interface retains c
const badInterface: BInterface = {};
// @ts-expect-error renamed object type retains f
const badType: AType = {};
// @ts-expect-error original conflicting names are not public entry exports
import { Klass } from "__ENTRY__";
