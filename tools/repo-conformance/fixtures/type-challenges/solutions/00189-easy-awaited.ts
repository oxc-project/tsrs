type MyAwaited<T> = T extends PromiseLike<infer Value> ? MyAwaited<Value> : T
