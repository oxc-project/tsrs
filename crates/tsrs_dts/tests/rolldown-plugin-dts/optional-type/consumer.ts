import type { DateRange } from '__ENTRY__';
const empty: DateRange = [];
const start: DateRange = [new Date()];
const open: DateRange = [undefined, new Date()];
// @ts-expect-error the optional tuple elements remain Date
const wrong: DateRange = ['today'];
// @ts-expect-error the tuple has at most two elements
const tooLong: DateRange = [new Date(), new Date(), new Date()];
