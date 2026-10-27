import { parse } from "__ENTRY__";
import type { Lexer } from "__ENTRY__";
import type { Node, Token } from "ext-pkg";
const node = parse();
const result: Node = node;
const value: string = node.value;
const lexer: Lexer = { current: { kind: "token", text: "word" } };
const token: Token = lexer.current;
const text: string = lexer.current.text;
// @ts-expect-error the namespace-imported Node retains its member type
const wrongNodeValue: number = node.value;
// @ts-expect-error the named-imported Token retains its member type
const wrongTokenText: number = lexer.current.text;
// @ts-expect-error the namespace and named imports resolve to distinct exports
const swapped: Lexer = { current: node };
// @ts-expect-error the external Node must not become an entry export
import type { Node as LeakedNode } from "__ENTRY__";
// @ts-expect-error the external Token must not become an entry export
import type { Token as LeakedToken } from "__ENTRY__";
