/** @template T */
class Base {
    /** @param {T} value */
    constructor(value) {
        this.value = value;
    }
}

/** @extends {Base<{ [K in "key"]: K }>} */
class ExtendsMapped extends Base {}

/** @augments {Base<{ [K in "key"]: K }>} */
class AugmentsMapped extends Base {}

new ExtendsMapped({ key: "key" });
new AugmentsMapped({ key: "key" });
new ExtendsMapped({ key: 123 });
new AugmentsMapped({ key: 123 });
