import type { API } from '__ENTRY__';
const api: API = {item:{value:'ok'}};
// @ts-expect-error external member remains string
const wrong: API = {item:{value:1}};
