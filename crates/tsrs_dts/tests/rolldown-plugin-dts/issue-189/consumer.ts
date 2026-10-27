import { MyClass } from '__ENTRY__';
const instance: MyClass = new MyClass();
const ctor: new () => MyClass = MyClass;
// @ts-expect-error the null-derived class has no declared constructor parameters
new MyClass(1);
// @ts-expect-error the class constructor cannot be called as a function
MyClass();
