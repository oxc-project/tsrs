import Child from './index.js';
const props: InstanceType<typeof Child>['$props'] = { message: 'hello' };
// @ts-expect-error message is required
const missing: InstanceType<typeof Child>['$props'] = {};
declare const child: InstanceType<typeof Child>;
child.$emit('change', 'hello');
// @ts-expect-error event argument is string
child.$emit('change', 123);
child.$slots.default({ label: 'hello' });
// @ts-expect-error slot label is string
child.$slots.default({ label: 123 });
export { props, missing };
