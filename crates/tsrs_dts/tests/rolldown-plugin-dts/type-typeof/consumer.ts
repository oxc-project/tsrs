import { typeQuery } from "__ENTRY__";
const result: {} = typeQuery();
// @ts-expect-error typeof query retains an empty interface result, not number
const number: number = typeQuery();
// @ts-expect-error no arguments are accepted
typeQuery(1);
