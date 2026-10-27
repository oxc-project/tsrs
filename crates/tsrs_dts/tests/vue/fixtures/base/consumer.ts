import { Child, Parent } from './index.js';
type Props = InstanceType<typeof Child>['$props'];
const good: Props = { message: 'hello', count: 1 };
// @ts-expect-error required message
const missing: Props = {};
// @ts-expect-error message is string
const wrong: Props = { message: 123 };
// @ts-expect-error count is number
const wrongCount: Props = { message: 'hello', count: 'one' };
declare const child: InstanceType<typeof Child>;
child.$emit('change', 'hello');
// @ts-expect-error event payload is string
child.$emit('change', 123);
// @ts-expect-error event name
child.$emit('unknown', 'hello');
child.$slots.default({ label: 'hello' });
// @ts-expect-error slot prop is string
child.$slots.default({ label: 123 });
const parent: InstanceType<typeof Parent>['$props'] = { title: 'hello' };
// @ts-expect-error parent title is string
const wrongParent: InstanceType<typeof Parent>['$props'] = { title: false };
export { good, missing, wrong, wrongCount, parent, wrongParent };
