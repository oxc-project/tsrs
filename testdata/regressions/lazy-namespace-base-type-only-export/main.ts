import type { Box, Two } from './model';
interface Source { a: number; live: boolean; aliasToken: string }
declare const source: Source;
const missing: Box<number> = source;
declare const box: Box<number>;
const inherited: number = box.hiddenToken;
const renamed: string = box.renamed;
const shape: unknown = box.Shape;
declare const two: Two<number>;
const first: string = two.hiddenToken;
