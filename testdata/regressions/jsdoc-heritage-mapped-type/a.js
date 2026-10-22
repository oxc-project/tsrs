/** @template T */
class Base {}

/** @extends {Base<{ [K in string]: K }>} */
class ExtendsMapped extends Base {}

/** @augments {Base<{ [K in string]: K }>} */
class AugmentsMapped extends Base {}

/** @typedef {{ open: (value: number) => void, close: () => void }} Events */

/** @extends {Base<{ [K in keyof Events]: Parameters<Events[K]> }>} */
class ExtendsKeyof extends Base {}

/** @augments {Base<{ [K in keyof Events]: Parameters<Events[K]> }>} */
class AugmentsKeyof extends Base {}

/** @extends {Base<{ [K in "first" | "second" as `get${K}`]?: K }>} */
class RenamedKeys extends Base {}

/** @extends {Base<{ [K in string]: { [P in K]: P } }>} */
class NestedMapped extends Base {}

/** @type {{ [K in string]: K }} */
let value = {};
