import { foo } from "__ENTRY__";
const literal: "hello" = foo("*hello*");
// @ts-expect-error delimiter pattern remains required
foo("hello");
// @ts-expect-error inferred payload is not a different literal
const wrong: "other" = foo("*hello*");
