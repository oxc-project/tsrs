import { MyComponent, type MyComponentProps } from '__ENTRY__';
const props: MyComponentProps = { foo: 'ok', id: 'main' };
const component = new MyComponent(props);
const value: string = component.props.foo;
// @ts-expect-error required foo prop
const wrong: MyComponentProps = { id: 'main' };
