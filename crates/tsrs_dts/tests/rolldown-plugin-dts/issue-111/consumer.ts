import { Stuff, OtherStuff } from '__ENTRY__';
const left: string = Stuff.id;
const right: string = OtherStuff.id;
const shape: typeof Stuff = { id: 'id' };
// @ts-expect-error retain each merged value's interface member type
const wrong: typeof OtherStuff = { id: 1 };
// @ts-expect-error the object value is not callable
Stuff();
