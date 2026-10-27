import { a, b } from '__ENTRY__';
const left: string = a('left');
const right: string = b('right');
a.staticProp = 'left';
b.staticProp = 'right';
// @ts-expect-error renamed merge keeps a's call signature
a(1);
// @ts-expect-error renamed merge keeps b's namespace members
b.staticProp = 1;
// @ts-expect-error unused merged declaration must not be exported
import { unused } from '__ENTRY__';
