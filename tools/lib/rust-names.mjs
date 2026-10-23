// Rust constant names are independent of upstream identifiers and serialized labels.
export const screamingSnakeCase = (name) =>
    name
        .replace(/JSDoc/g, "Jsdoc")
        .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
        .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
        .toUpperCase();

export function constantNames(names) {
    const result = new Map(), seen = new Map();
    for (const name of names) {
        const rust = screamingSnakeCase(name);
        if (seen.has(rust)) throw new Error(`Rust constant name collision: ${seen.get(rust)} and ${name} -> ${rust}`);
        result.set(name, rust);
        seen.set(rust, name);
    }
    return result;
}
