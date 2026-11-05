type Choice<K extends string> = (Record<K, 0> & (() => void)) | (Record<K, 1> & (() => void));
declare function use(callback: () => void): void;
declare function use<
  A extends Choice<'a'>,
  B extends Choice<'b'>,
  C extends Choice<'c'>,
  D extends Choice<'d'>,
  E extends Choice<'e'>,
  F extends Choice<'f'>,
  G extends Choice<'g'>,
  H extends Choice<'h'>,
  I extends Choice<'i'>,
  J extends Choice<'j'>,
  K extends Choice<'k'>,
  L extends Choice<'l'>,
  M extends Choice<'m'>,
  N extends Choice<'n'>
>(callback: A & { kind: 'a' } & B & C & D & E & F & G & H & I & J & K & L & M & N & { kind: 'b' }): void;
use(() => {});
