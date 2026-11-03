// Loaded with --import: every `tsrs/sync` import outside this package resolves to its facade, so code written for
// the subprocess API (here, the vendored upstream suites) runs its sessions in this process.
import module from "node:module";

const facade = new URL("../src-js/index.mjs", import.meta.url).href;

module.registerHooks({
    resolve(specifier, context, nextResolve) {
        if (specifier === "tsrs/sync" && context.parentURL !== facade) return { url: facade, shortCircuit: true };
        return nextResolve(specifier, context);
    },
});
