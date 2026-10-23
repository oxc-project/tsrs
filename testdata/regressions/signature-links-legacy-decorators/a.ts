function decorate(..._args: any[]): void {}

@decorate
class Guards {
    // Legacy method and parameter decorator signatures use the declaration's cache.
    @decorate
    isString(@decorate value: unknown): value is string {
        return typeof value === "string";
    }

    @decorate
    assertString(value: unknown): asserts value is string {
        if (typeof value !== "string") throw new Error("Expected string");
    }
}

const guards: Guards = new Guards();
declare let value: unknown;

if (guards.isString(value)) {
    value.toUpperCase();
    const bad: number = value;
}
guards.assertString(value);
value.toUpperCase();
const bad: number = value;

class Invalid {
    @decorate
    constructor() {}
}
