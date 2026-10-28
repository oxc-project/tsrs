import autobind from '__ENTRY__';
const decorator: ClassDecorator | MethodDecorator = autobind();
const result: void = autobind(function C() {});
const descriptor: PropertyDescriptor = autobind({}, 'method', {});
// @ts-expect-error no two-argument overload
autobind({}, 'method');
// @ts-expect-error constructor overload requires Function
autobind(1);
