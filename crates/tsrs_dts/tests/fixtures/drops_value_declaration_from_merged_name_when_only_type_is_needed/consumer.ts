import type { UsesFoo } from "__ENTRY__";
const good: UsesFoo = { value: "ok" };
// @ts-expect-error merged type keeps its property type
const bad: UsesFoo = { value: 1 };
// @ts-expect-error supporting merged name remains private
import { Foo } from "__ENTRY__";
