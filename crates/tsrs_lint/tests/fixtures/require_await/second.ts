const naïve = '🦀';
async function fresh(): globalThis.Promise<number> { return 1; }
async function awaitLoop(values: AsyncIterable<number>) { for await (const value of values) {} }
async function last() { return; }
