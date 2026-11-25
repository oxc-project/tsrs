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

const inferred: string = one();
