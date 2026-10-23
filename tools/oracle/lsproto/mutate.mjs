#!/usr/bin/env node
import { createInterface } from "node:readline";
import { args, random, main } from "../../lib/cli.mjs";
function* paths(v, prefix = []) {
    yield prefix;
    if (v && typeof v === "object")
        for (const [k, x] of Object.entries(v)) yield* paths(x, [...prefix, Array.isArray(v) ? Number(k) : k]);
}
function replace(v, path, value) {
    if (!path.length) return value;
    const out = Array.isArray(v) ? [...v] : { ...v };
    out[path[0]] = replace(v[path[0]], path.slice(1), value);
    return out;
}
function drop(v, path) {
    if (path.length === 1) {
        const out = Array.isArray(v) ? [...v] : { ...v };
        if (Array.isArray(out)) out.splice(path[0], 1);
        else delete out[path[0]];
        return out;
    }
    return replace(v, path.slice(0, 1), drop(v[path[0]], path.slice(1)));
}
main(async () => {
    const a = args("Mutate Type<TAB>JSON lines read from stdin.", { seed: 1 }),
        rng = random(a.seed),
        wrong = (v) =>
            typeof v === "boolean"
                ? "true"
                : typeof v === "number"
                  ? rng() < 0.5
                      ? "x"
                      : 1.5
                  : typeof v === "string"
                    ? 5
                    : Array.isArray(v)
                      ? {}
                      : v && typeof v === "object"
                        ? []
                        : false;
    for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
        const i = line.indexOf("\t"),
            name = line.slice(0, i),
            text = line.slice(i + 1),
            v = JSON.parse(text),
            all = [...paths(v)].filter((p) => p.length),
            object = all.filter((p) => typeof p.at(-1) === "string"),
            emit = (value) => console.log(`${name}\t${JSON.stringify(value)}`);
        console.log(line);
        if (object.length) {
            emit(drop(v, rng.choice(object)));
            emit(replace(v, rng.choice(object), null));
        }
        if (all.length) {
            const p = rng.choice(all),
                target = p.reduce((x, k) => x[k], v);
            emit(replace(v, p, wrong(target)));
        }
        if (v && !Array.isArray(v) && typeof v === "object") {
            emit({ ...v, someUnknownField: { nested: [1, null] } });
            emit(Object.fromEntries(Object.entries(v).reverse()));
        }
        emit(wrong(v));
        if (rng() < 0.1) emit(null);
    }
});
