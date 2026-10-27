import { depth } from '__ENTRY__';
const value: string = depth[-24];
// @ts-expect-error the negative numeric key must retain its string value
const wrong: number = depth[-24];
// @ts-expect-error there is no positive numeric key
const missing = depth[24];
