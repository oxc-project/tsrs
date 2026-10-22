export {};
declare const café: string;
const caractère = 'é';
const alias = caractère;
café /* 汉 */ [0] !== (alias);
café?.['charAt'](café.length - 1) === '👍';
café.slice(0, `é`.length) == `é`;
café.substring(-1) === 'é';
café?.['indexOf']('é') === -0;
café.lastIndexOf('é') === (café.length - 1);
const pattern = /^é\n\0\"/du;
const regexAlias = pattern;
regexAlias?.['test']((café + '汉'));
café.match(regexAlias) == void 0;
café.match(regexAlias) === undefined;
/^é\²/.test(café);
/^é\١/.test(café);
new RegExp('^é', 'sdu').test(café);
/^é/i.test(café);
/^é/m.test(café);
/^é/g.test(café);
/^é/y.test(café);
/^é$/.test(café);
/^é\d/.test(café);
function check<T extends string & { brand: true }>(text: T) {
    text.charAt(0) === 'é';
}
