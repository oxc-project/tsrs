import initialize from "__ENTRY__";
const promise: Promise<void> = initialize({ count: 1, name: "x" }, { count: async () => 2, name: async () => "y" });
// @ts-expect-error mapped initialization requires all object keys
initialize({ count: 1 }, {});
// @ts-expect-error initializer result follows corresponding object property
initialize({ count: 1 }, { count: async () => "wrong" });
