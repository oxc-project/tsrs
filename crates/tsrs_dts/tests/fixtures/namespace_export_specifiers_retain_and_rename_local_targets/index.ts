export { A } from './a.js'; export { B } from './b.js'; import { Item } from './a.js'; declare global { namespace Outer.Inner { export { Item as GlobalItem }; } }
