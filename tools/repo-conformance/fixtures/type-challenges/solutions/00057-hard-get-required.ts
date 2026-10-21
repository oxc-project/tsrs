type GetRequired<T> = { [K in keyof T as {} extends Pick<T, K> ? never : K]: T[K] }
