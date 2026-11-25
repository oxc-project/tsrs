async function* one() {
    yield { then() {} };
}

async function* twice() {
    yield { then() {} };
    yield { then() {} };
}

const contextual: () => AsyncGenerator<number> = async function* () {
    yield { then() {} };
};

interface SelfThenable {
    then(onfulfilled: (value: SelfThenable) => void): void;
}
declare const selfThenable: SelfThenable;
async function* circular() {
    yield selfThenable;
}

declare const thenables: AsyncIterable<{ then(): void }>;
async function* delegated() {
    yield* thenables;
}

const inferred: string = one();
const inferredCircular: string = circular();
const inferredDelegated: string = delegated();
