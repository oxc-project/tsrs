export { Item } from './barrel.js'; export type Nested = import('./barrel.js').ns.Item; export type Module = typeof import('./barrel.js').ns;
