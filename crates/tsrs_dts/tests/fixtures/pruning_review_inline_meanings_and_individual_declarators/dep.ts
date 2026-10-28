export interface Foo { value: string } export const Foo = { DROP_INLINE_VALUE: true }; export const used: string = 'ok', unused: { DROP_DECLARATOR: true } = { DROP_DECLARATOR: true };
