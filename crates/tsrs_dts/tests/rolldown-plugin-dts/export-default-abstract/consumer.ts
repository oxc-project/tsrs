import MemberInfo from '__ENTRY__';
declare const info: MemberInfo;
const name: string = info.name;
const type: {} = info.declaringType;
const member: {} = info.memberType;
// @ts-expect-error abstract class cannot be constructed
new MemberInfo();
// @ts-expect-error readonly member remains readonly
info.name = 'x';
// @ts-expect-error abstract members remain mandatory
class Missing extends MemberInfo {}
