import type { AbstractConstructor } from "__ENTRY__";
declare abstract class Implementation {
  abstract someMethod(): {};
  badda(): void;
  member: {};
}
const constructor: AbstractConstructor<Implementation> = Implementation;
// @ts-expect-error abstract constructor cannot be instantiated
new constructor();
// @ts-expect-error generic constraint retains inherited methods and member
type MissingShape = AbstractConstructor<{}>;
