import type { Module, Item, Only, Empty, EmptyAlias, Named } from '__ENTRY__';
import { EmptyTools } from '__ENTRY__';
const emptyValue: Empty = EmptyTools;
// @ts-expect-error empty namespace value has no exports
EmptyTools.absent;
declare const module: Module;
const value: string = module.default().value;
const weird: number = module['a-b'];
const item: Item = { value: 'ok' };
const only: Only.Item = item;
const named: Named = item;
const empty: Empty = {};
const emptyAlias: EmptyAlias = {};
declare const emptyKey: keyof Empty;
const noEmptyKey: never = emptyKey;
// @ts-expect-error missing module member must be rejected
module.absent;
// @ts-expect-error type-only namespace cannot supply runtime values
Only.default();
// @ts-expect-error empty module exports no members
empty.absent;
