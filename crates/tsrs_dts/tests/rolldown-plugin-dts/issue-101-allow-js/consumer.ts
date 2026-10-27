import { addNumber } from '__ENTRY__';
const numberResult: number = addNumber(1, 2);
const stringResult: string = addNumber('a', 'b');
// TypeScript 5.9's native declaration emit also makes both parameters required.
// Omitted JavaScript arguments are a source/declaration boundary limitation, recorded in the review.
// @ts-expect-error the inferred function has two parameters
addNumber(1, 2, 3);
