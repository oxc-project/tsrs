import { "\u007fexport" as fromQuoted } from "./quoted";
import { "\u007fexport" as fromMixed, other } from "./mixed";
import { "\u007fexport" as namespace } from "./namespace";

const quoted: string = fromQuoted;
const mixed: string = fromMixed;
const star: number = other;
const member: number = namespace.other;
