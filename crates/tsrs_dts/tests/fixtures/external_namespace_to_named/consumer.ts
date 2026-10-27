import { parse } from "__ENTRY__";
import type { Node } from "ext-pkg";
const node = parse();
const result: Node = node;
const kind: "node" = node.kind;
const value: string = node.value;
// @ts-expect-error the external namespace member retains its value type
const wrongValue: number = node.value;
// @ts-expect-error the return type retains the external Node shape
const wrongNode: ReturnType<typeof parse> = { kind: "node", value: 1 };
// @ts-expect-error the external support type must not become an entry export
import type { Node as LeakedNode } from "__ENTRY__";
