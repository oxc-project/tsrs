import { Foo } from "__ENTRY__";
const instance: Foo = new Foo();
class Derived extends Foo {
  useProtected(): void { const b = this.b; this.mb(); }
  // @ts-expect-error private members remain inaccessible to subclasses
  usePrivate(): void { this.a; }
}
// @ts-expect-error private field is inaccessible
instance.a;
// @ts-expect-error private method marker is inaccessible
instance.ma;
// @ts-expect-error protected field is inaccessible outside subclasses
instance.b;
// @ts-expect-error protected method is inaccessible outside subclasses
instance.mb();
// @ts-expect-error private/protected members preserve nominal class identity
const structural: Foo = {};
// @ts-expect-error supporting class is not a public export
import type { B } from "__ENTRY__";
