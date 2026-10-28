import { Ns, tools } from '__ENTRY__';
import type { Only } from '__ENTRY__';
const value: number = new Ns.Ref().value;
const nested: number = new tools.self.Ns.Ref().value;
const item: Only.Ns.Box = { ref: new Ns.Ref() };
// @ts-expect-error nominal identity must remain
const bad: Ns.Box = { ref: { value: 1 } };
// @ts-expect-error type-only class export cannot become a value
new tools.Client();
