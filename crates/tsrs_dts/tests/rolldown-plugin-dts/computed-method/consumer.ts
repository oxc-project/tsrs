import { Test } from '__ENTRY__';
import { inspect } from 'util';
declare const value: Test;
const a: string = value[inspect.custom]();
const b: string = value.b();
const c: string = value.deep;
// @ts-expect-error computed method returns string
const wrong: number = value.b();
