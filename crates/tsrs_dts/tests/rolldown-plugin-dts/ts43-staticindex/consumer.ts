import { Foo } from "__ENTRY__";
const greeting: string = Foo.hello;
const count: number = Foo.world;
const indexed: {} = Foo["arbitrary"];
Foo["arbitrary"] = {};
// @ts-expect-error named static property is still string
Foo.hello = 1;
// @ts-expect-error static index excludes null
Foo["arbitrary"] = null;
// @ts-expect-error static index signature does not become an instance index
new Foo()["arbitrary"];
