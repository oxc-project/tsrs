import * as api from '__ENTRY__';
const empty: keyof typeof api extends never ? true : false = true;
// @ts-expect-error unused private binding must not become a public export
import { a } from '__ENTRY__';
