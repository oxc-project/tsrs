function keepMethod<This, Value extends (this: This, ...args: any[]) => any>(
    value: Value,
    _context: ClassMethodDecoratorContext<This, Value>,
): Value {
    return value;
}

class Guards {
    // The method's resolved signature and runtime decorator signature must coexist.
    @keepMethod
    isString(value: unknown): value is string {
        return typeof value === "string";
    }

    @keepMethod
    assertString(value: unknown): asserts value is string {
        if (typeof value !== "string") throw new Error("Expected string");
    }
}

const guards: Guards = new Guards();
declare let value: unknown;

// Calls need both their resolved signature and the effects used for narrowing.
if (guards.isString(value)) {
    value.toUpperCase();
    const bad: number = value;
}
guards.assertString(value);
value.toUpperCase();
const bad: number = value;

class Invalid {
    @keepMethod
    constructor() {}
}
