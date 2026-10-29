// tsrs/sync with the API session in this process: the same client and `API`, but its channel serves the
// `tsrs --api` session on a thread here instead of spawning it. Filesystem callbacks run on the calling thread.
import { createRequire } from "node:module";
import { API as SubprocessAPI } from "tsrs/sync";

const { SyncRpcChannel } = createRequire(import.meta.url)("./tsrs.node");

// The native channel reports a failed callback with the thrown object's `message`. Below Node-API 10 napi-rs cannot
// hold a thrown primitive, so callbacks rethrow one as an `Error` of `String(value)`, as syncChannel.ts reports it.
class Channel extends SyncRpcChannel {
    registerCallback(name, callback) {
        super.registerCallback(name, (...args) => {
            try {
                return callback(...args);
            }
            catch (error) {
                throw Object(error) === error ? error : new Error(String(error));
            }
        });
    }
}

export * from "tsrs/sync";

/** `new API(options)` returns a tsrs/sync `API` whose channel is in this process; prototype, statics and instanceof match. */
export function API(options = {}) {
    return new SubprocessAPI({ ...options, createChannel: args => new Channel(args) });
}
API.prototype = SubprocessAPI.prototype;
Object.setPrototypeOf(API, SubprocessAPI);
