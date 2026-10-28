import { greet, add } from '__ENTRY0__';
import { add as utilityAdd, subtract } from '__ENTRY1__';
const message: string = greet('world');
const sum: number = utilityAdd(add(1, 2), subtract(4, 1));
// @ts-expect-error dependency-only member stays off index entry
import { subtract as privateSubtract } from '__ENTRY0__';
