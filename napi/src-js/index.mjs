// tsrs/sync with the API session in this process: the same client and `API`, but its channel serves the
// `tsrs --api` session on a thread here instead of spawning it. Filesystem callbacks run on the calling thread.
import { createRequire } from "node:module";
import { API as SubprocessAPI } from "tsrs/sync";

const { SyncRpcChannel } = createRequire(import.meta.url)("./tsrs.node");

export * from "tsrs/sync";

/** `new API(options)` returns a tsrs/sync `API` whose channel is in this process; prototype, statics and instanceof match. */
export function API(options = {}) {
    return new SubprocessAPI({ ...options, createChannel: args => new SyncRpcChannel(args) });
}
API.prototype = SubprocessAPI.prototype;
Object.setPrototypeOf(API, SubprocessAPI);
