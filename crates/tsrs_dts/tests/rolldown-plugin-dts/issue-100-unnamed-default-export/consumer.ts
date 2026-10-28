import initialize from '__ENTRY__';
const result: Promise<void> = initialize({ n: 1, text: '' }, {
  n: async () => 2,
  text: async () => 'ready',
});
// @ts-expect-error each initializer must resolve to the corresponding property type
initialize({ n: 1 }, { n: async () => 'wrong' });
// @ts-expect-error every property needs an initializer
initialize({ n: 1 }, {});
