import { KEYWORDS, tokenize, pairBrackets } from "./tslex.mjs";
export const MUTATORS =
    "prop_rename prop_drop str_change swap_args remove_arg add_arg remove_await num_to_str delete_return rename_import_use eq_incompat remove_as generic_arg flip_optchain const_assign remove_case bad_method member_sig zod_swap prop_type_change optional_toggle remove_async drop_type_args remove_nullish".split(
        " ",
    );
const notCallees = new Set(
    "if for while switch catch function return typeof import await async with in of new void delete throw yield case else do keyof extends implements satisfies as is super constructor".split(
        " ",
    ),
);
export function candidates(file, src, rng) {
    const [t] = tokenize(src),
        [match, parent] = pairBrackets(t),
        edits = [],
        tx = (i) => t[i]?.text,
        add = (mutator, start, end, repl, note = "") => edits.push({ mutator, start, end, repl, note }),
        span = (name, a, b, repl, note) => {
            if (t[a] && t[b]) add(name, t[a].start, t[b].end, repl, note);
        },
        typo = (s) =>
            s.length >= 5 && rng() < 0.45 ? s.slice(0, 1) + s.slice(2) : s + rng.choice(["s", "x", "Id", "2"]);
    function extent(i) {
        const p = parent[i];
        let j = i;
        while (
            j + 1 < t.length &&
            parent[j + 1] >= p &&
            !(parent[j + 1] === p && [",", ";", "=", "}", ")", "=>"].includes(tx(j + 1)))
        )
            j++;
        return j;
    }
    const imports = new Set();
    let inImport = false;
    for (let i = 0; i < t.length; i++) {
        if (tx(i) === "import") inImport = true;
        if (inImport && t[i].kind === "id" && !KEYWORDS.has(tx(i))) imports.add(tx(i));
        if (inImport && (tx(i) === ";" || t[i].kind === "str")) inImport = false;
    }
    for (let i = 0; i < t.length; i++) {
        const tok = t[i],
            text = tok.text;
        if (["await", "async"].includes(text))
            add(
                text === "await" ? "remove_await" : "remove_async",
                tok.start,
                t[i + 1]?.start ?? tok.end,
                "",
                `remove ${text}`,
            );
        if (tok.kind === "num") span("num_to_str", i, i, JSON.stringify(text));
        if (
            tok.kind === "str" &&
            !["from", "import", "require", "module"].includes(tx(i - 1)) &&
            tx(i - 2) !== "require" &&
            tx(i - 2) !== "import"
        )
            span("str_change", i, i, text[0] + text.slice(1, -1) + "x" + text.at(-1));
        if (
            tok.kind === "id" &&
            imports.has(text) &&
            !["import", "{", ",", "as", "."].includes(tx(i - 1)) &&
            tx(i + 1) !== ":"
        )
            span("rename_import_use", i, i, typo(text));
        if (["===", "!==", "==", "!="].includes(text) && t[i + 1] && ["str", "num", "id"].includes(t[i + 1].kind))
            span("eq_incompat", i + 1, i + 1, t[i + 1].kind === "str" ? "12345" : '"__mut__"');
        if (text === "as" && t[i + 1]) span("remove_as", i, extent(i + 1), "");
        if (text === "?.") span("flip_optchain", i, i, ".");
        if (text === "." && t[i + 1]?.kind === "id") {
            span("flip_optchain", i, i, "?.");
            span("bad_method", i + 1, i + 1, typo(tx(i + 1)));
            if (tx(i - 1) === "z" && ["string", "number", "boolean", "date", "any", "unknown"].includes(tx(i + 1)))
                span("zod_swap", i + 1, i + 1, tx(i + 1) === "string" ? "number" : "string");
        }
        if (text === "??" && t[i + 1]) span("remove_nullish", i, extent(i + 1), "");
        if (text === "return" && t[i + 1] && !t[i + 1].nl_before && tx(i + 1) !== ";")
            span("delete_return", i + 1, extent(i + 1), "");
        if (text === "const" && t[i + 1]?.kind === "id" && tx(i + 2) === "=") {
            const end = extent(i + 3);
            if (tx(end + 1) === ";") add("const_assign", t[end + 1].end, t[end + 1].end, `\n${tx(i + 1)} = undefined;`);
        }
        if (text === "case") {
            let j = i + 1;
            while (j < t.length && !(parent[j] === parent[i] && ["case", "default", "}"].includes(tx(j)))) j++;
            if (j > i) span("remove_case", i, j - 1, "");
        }
        if (text === "?" && tx(i + 1) === ":" && t[i - 1]?.kind === "id") span("optional_toggle", i, i, "");
        if (text === ":" && t[i - 1]?.kind === "id" && ["string", "number", "boolean"].includes(tx(i + 1))) {
            span("prop_type_change", i + 1, i + 1, tx(i + 1) === "string" ? "number" : "string");
            add("optional_toggle", tok.start, tok.start, "?");
            span("member_sig", i + 1, i + 1, "never");
        }
        if (text === ":" && t[i - 1]?.kind === "id") {
            const p = parent[i];
            if (p >= 0 && tx(p) === "{" && ["=", "(", ",", "[", "return", "=>"].includes(tx(p - 1))) {
                span("prop_rename", i - 1, i - 1, typo(tx(i - 1)));
                let end = extent(i + 1);
                if (tx(end + 1) === ",") end++;
                span("prop_drop", i - 1, end, "");
            }
        }
        if (
            text === "(" &&
            match[i] > i &&
            !notCallees.has(tx(i - 1)) &&
            (t[i - 1]?.kind === "id" || [")", "]"].includes(tx(i - 1))) &&
            !["{", ":", "=>"].includes(tx(match[i] + 1)) &&
            !["function", "get", "set", "async"].includes(tx(i - 2))
        ) {
            const close = match[i],
                bounds = [
                    i,
                    ...t.map((_, j) => j).filter((j) => j > i && j < close && parent[j] === i && tx(j) === ","),
                    close,
                ],
                arguments_ = bounds
                    .slice(0, -1)
                    .map((b, j) => [b + 1, bounds[j + 1] - 1])
                    .filter(([s, e]) => s <= e);
            if (arguments_.length >= 2) {
                const [a, b] = arguments_,
                    source = (range) => src.slice(t[range[0]].start, t[range[1]].end);
                add("swap_args", t[a[0]].start, t[b[1]].end, `${source(b)}, ${source(a)}`);
            }
            for (let j = 0; j < arguments_.length; j++) {
                const [s, e] = arguments_[j];
                span("remove_arg", j ? bounds[j] : s, j ? e : arguments_.length > 1 ? bounds[1] : e, "");
            }
            add("add_arg", t[close].start, t[close].start, arguments_.length ? ", undefined" : "undefined");
        }
        if (text === "<" && t[i - 1]?.kind === "id") {
            let depth = 1,
                j = i + 1;
            while (j < t.length && depth) {
                if (tx(j) === "<") depth++;
                if (tx(j) === ">") depth--;
                j++;
            }
            if (!depth && ["(", ".", ";", ",", ")", "=", "{"].includes(tx(j))) {
                span("drop_type_args", i, j - 1, "");
                if (i + 1 < j - 1) span("generic_arg", i + 1, j - 2, "never");
            }
        }
    }
    return edits;
}
export function pickMutation(file, src, rng, weights = {}) {
    const available = new Map();
    for (const e of candidates(file, src, rng)) {
        if (!available.has(e.mutator)) available.set(e.mutator, []);
        available.get(e.mutator).push(e);
    }
    const names = [...available.keys()].sort().filter((n) => (weights[n] ?? 1) > 0);
    if (!names.length) return null;
    let choice = rng() * names.reduce((s, n) => s + (weights[n] ?? 1), 0);
    const name = names.find((n) => (choice -= weights[n] ?? 1) < 0) ?? names.at(-1);
    return rng.choice(available.get(name));
}
