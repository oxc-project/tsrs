import { thisType } from "__ENTRY__";
declare const receiver: ThisParameterType<typeof thisType>;
const nested: typeof receiver = receiver.a;
thisType.call(receiver);
// @ts-expect-error receiver retains recursive a member
thisType.call({});
// @ts-expect-error this parameter is not a positional parameter
thisType(receiver);
