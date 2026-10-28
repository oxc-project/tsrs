import { MyComponent } from '__ENTRY__';
import type { MyComponentProps } from '__ENTRY__';
const props: MyComponentProps = { foo: {}, bar: 'x' };
const component = new MyComponent();
component.props = props;
const bar: string = component.props.bar;
// @ts-expect-error namespace heritage retains required field
const missing: MyComponentProps = { bar: 'x' };
// @ts-expect-error inherited generic field keeps type
component.props.bar = 1;
