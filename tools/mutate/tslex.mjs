// A lexical scanner for mutations: comments, strings, regexes and template parts stay opaque.
export const KEYWORDS = new Set(
    "abstract as async await break case catch class const continue debugger declare default delete do else enum export extends false finally for from function get if implements import in instanceof interface is keyof let new null of private protected public readonly return satisfies set static super switch this throw true try type typeof undefined var void while with yield infer unique override accessor using module namespace asserts never unknown any string number boolean symbol object bigint".split(
        " ",
    ),
);
const puncts = [
    "...",
    "===",
    "!==",
    "**=",
    "<<=",
    "&&=",
    "||=",
    "??=",
    "=>",
    "==",
    "!=",
    "<=",
    "&&",
    "||",
    "??",
    "?.",
    "++",
    "--",
    "+=",
    "-=",
    "*=",
    "/=",
    "%=",
    "&=",
    "|=",
    "^=",
    "<<",
    "**",
];
const regexPunct = new Set(
        "( , = : [ ! & | ? { ; + - * % < > ~ ^ => == === != !== && || ?? += -= *= %= &= |= ^= <= ... **".split(" "),
    ),
    regexKW = new Set("return typeof case do else in of new delete void throw instanceof yield await".split(" "));
export class LexError extends Error {}
export function tokenize(src) {
    const toks = [],
        comments = [],
        braces = [];
    let i = 0,
        line = 0,
        nl = true;
    function template(j) {
        while (j < src.length) {
            if (src[j] === "\\") j += 2;
            else if (src[j] === "`") return [j + 1, true];
            else if (src.startsWith("${", j)) return [j + 2, false];
            else j++;
        }
        throw new LexError("unterminated template");
    }
    while (i < src.length) {
        const c = src[i];
        if (c === "\n") {
            line++;
            nl = true;
            i++;
            continue;
        }
        if (/\s|\uFEFF/.test(c)) {
            i++;
            continue;
        }
        if (src.startsWith("//", i)) {
            let j = src.indexOf("\n", i);
            if (j < 0) j = src.length;
            comments.push([i, j]);
            i = j;
            continue;
        }
        if (src.startsWith("/*", i)) {
            const j = src.indexOf("*/", i + 2);
            if (j < 0) throw new LexError("unterminated comment");
            const n = src.slice(i, j + 2).split("\n").length - 1;
            line += n;
            nl ||= n > 0;
            comments.push([i, j + 2]);
            i = j + 2;
            continue;
        }
        const start = i,
            startLine = line;
        let end, kind;
        if (/[\p{L}_$#]/u.test(c) || c.charCodeAt(0) > 127) {
            const m = /^[\p{L}\p{N}_$#]+/u.exec(src.slice(i));
            end = i + (m?.[0].length ?? 1);
            kind = "id";
        } else if (/\d/.test(c) || (c === "." && /\d/.test(src[i + 1] ?? ""))) {
            const m = /^(?:0[xXoObB][\da-zA-Z_]+|(?:\d[\d_]*(?:\.[\d_]*)?|\.[\d_]+)(?:[eE][+-]?\d+)?n?)/.exec(
                src.slice(i),
            );
            end = i + m[0].length;
            kind = "num";
        } else if (c === '"' || c === "'") {
            end = i + 1;
            while (end < src.length && src[end] !== c) {
                if (src[end] === "\\") end++;
                else if (src[end] === "\n") throw new LexError("newline in string");
                end++;
            }
            if (end >= src.length) throw new LexError("unterminated string");
            end++;
            kind = "str";
        } else if (c === "`" || (c === "}" && braces.at(-1) === "t")) {
            const continuation = c === "}";
            if (continuation) braces.pop();
            const [j, done] = template(i + 1);
            end = j;
            kind = continuation ? (done ? "tmpl_tail" : "tmpl_mid") : done ? "tmpl_full" : "tmpl_head";
            if (!done) braces.push("t");
        } else if (
            c === "/" &&
            (!toks.length ||
                (toks.at(-1).kind === "punct" && regexPunct.has(toks.at(-1).text)) ||
                (toks.at(-1).kind === "id" && regexKW.has(toks.at(-1).text)))
        ) {
            end = i + 1;
            let inClass = false,
                closed = false;
            while (end < src.length) {
                const d = src[end];
                if (d === "\\") {
                    end += 2;
                    continue;
                }
                if (d === "\n") throw new LexError("newline in regex");
                if (inClass) {
                    if (d === "]") inClass = false;
                } else if (d === "[") inClass = true;
                else if (d === "/") {
                    closed = true;
                    end++;
                    break;
                }
                end++;
            }
            if (!closed) throw new LexError("unterminated regex");
            while (end < src.length && /[\w]/.test(src[end])) end++;
            kind = "regex";
        } else {
            const p =
                c === "?" && src.startsWith("?.", i) && /\d/.test(src[i + 2] ?? "")
                    ? "?"
                    : (puncts.find((p) => src.startsWith(p, i)) ?? c);
            end = i + p.length;
            kind = "punct";
            if (p === "{") braces.push("b");
            else if (p === "}" && braces.length) braces.pop();
        }
        const text = src.slice(start, end);
        toks.push({ kind, text, start, end, line: startLine, nl_before: nl });
        line += text.split("\n").length - 1;
        i = end;
        nl = false;
    }
    return [toks, comments];
}
export function pairBrackets(toks) {
    const match = toks.map(() => -1),
        parent = toks.map(() => -1),
        stack = [],
        open = { "(": ")", "[": "]", "{": "}" };
    toks.forEach((t, i) => {
        parent[i] = stack.at(-1) ?? -1;
        if ((t.kind === "punct" && t.text in open) || t.kind === "tmpl_head") stack.push(i);
        else if (
            (t.kind === "punct" && [")", "]", "}"].includes(t.text)) ||
            ["tmpl_mid", "tmpl_tail"].includes(t.kind)
        ) {
            if (!stack.length) throw new LexError("unbalanced close");
            const o = stack.pop(),
                ot = toks[o],
                valid = t.kind.startsWith("tmpl_")
                    ? ["tmpl_head", "tmpl_mid"].includes(ot.kind)
                    : ot.kind === "punct" && open[ot.text] === t.text;
            if (!valid) throw new LexError("mismatched bracket");
            match[o] = i;
            match[i] = o;
            parent[i] = stack.at(-1) ?? -1;
            if (t.kind === "tmpl_mid") stack.push(i);
        }
    });
    if (stack.length) throw new LexError("unclosed bracket");
    return [match, parent];
}
