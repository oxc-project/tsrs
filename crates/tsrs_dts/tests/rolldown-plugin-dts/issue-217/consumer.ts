import { types } from '__ENTRY__';
const dog: 'hi' = types.dog.example;
const example: types.Example<'custom'> = { example: 'custom' };
// @ts-expect-error retain the string generic constraint
const wrongConstraint: types.Example<number> = { example: 1 };
// @ts-expect-error retain the generic literal argument
const wrongValue: types.Example<'custom'> = { example: 'other' };
