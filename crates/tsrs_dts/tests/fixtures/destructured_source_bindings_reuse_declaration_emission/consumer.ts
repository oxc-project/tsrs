import { label, first, rest } from '__ENTRY__'; const s: string = label; const n: number = first; const count: number = rest.count; // @ts-expect-error
const bad: string = first;
