export interface Item { value: string } export default function create(): Item { return { value: 'ok' } } const weird = 1; export { weird as 'a-b' };
