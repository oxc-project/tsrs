export { API } from './api.mjs'; import C = require('./client.cjs'); export type Client = C; export type Module = typeof import('./api.mjs');
