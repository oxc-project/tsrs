import { Cache, Cache2, uniqueId } from '__ENTRY__';
import type { CacheInfo } from '__ENTRY__';
const cache: Cache = Cache();
const other: Cache2 = Cache2();
const info: CacheInfo = { id: 1 };
const added: boolean = other.add(info);
const destroyed: void = cache.destroy();
const otherDestroyed: void = other.destroy();
const id: string = uniqueId();
const prefixed: string = uniqueId('prefix');
// @ts-expect-error retain CacheInfo.id's numeric annotation
other.add({ id: 'wrong' });
// @ts-expect-error prefix is optional string
uniqueId(1);
// @ts-expect-error Cache and Cache2 retain distinct member sets
cache.add(info);
