export {};
declare const café: { count: number };
declare const inconnu: unknown;
declare const rien: never;
declare const anyValue: any;
declare const nullable: string | null | undefined;
declare const nombres: readonly [bigint, number, boolean, (string | null)?];
declare const nested: readonly (readonly string[])[];
declare const empty: [];
declare const mixed: readonly [string, { count: number }];
declare const branded: string & { _brand: "é" };
class Custom extends Error {}
class CustomRegExp extends RegExp {}
declare function tag(strings: TemplateStringsArray, ...args: unknown[]): string;
const message = `pré ${ /* é */ café} ${inconnu} ${rien} ${anyValue} ${nullable} ${nombres} ${nested} ${empty} ${mixed} ${branded} ${new Custom()} ${new CustomRegExp()} ${/é/} ${true} ${12n}`;
const tagged = tag`pré ${café} ${`inner ${café}`}`;
function constrained<T extends readonly [string, number], U>(tuple: T, free: U) {
    return `${tuple} ${free}`;
}
