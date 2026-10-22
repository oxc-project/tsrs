export {};
declare const 值: string;
const 模式 = /café/;
模式.test(值);
值.indexOf() >= 0;
function f<T extends { includes: boolean }>(arg: T) { /word/.test(arg); }
const invalid = new RegExp("\\99999999999999999999999999");
invalid.test(值);
const trailing = new RegExp("word\\");
trailing.test(值);
const brackets = new RegExp("\\\\[a-z\\\\]");
brackets.test(值);
const reversed = new RegExp("\\\\[a--\\\\]");
reversed.test(值);
const escapedHyphen = new RegExp("\\\\[\\8-\\-\\\\]");
escapedHyphen.test(值);
const badProperty = new RegExp("\\\\[\\P\\\\]");
badProperty.test(值);
const badRange = new RegExp("\\\\[\\p-\\p\\\\]");
badRange.test(值);
declare const mapped: Record<'includes', boolean>;
/word/.test(mapped);
function mappedConstraint<T extends Record<'includes', boolean>>(arg: T) { /word/.test(arg); }
