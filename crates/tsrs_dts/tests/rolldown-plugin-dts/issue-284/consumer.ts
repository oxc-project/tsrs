import { MyInterface } from '__ENTRY__';
const instance: MyInterface = { a: 'instance' };
const staticValue: string = MyInterface.b;
// @ts-expect-error the interface member remains string
const wrongInstance: MyInterface = { a: 1 };
// @ts-expect-error the namespace member remains string
const wrongStatic: number = MyInterface.b;
