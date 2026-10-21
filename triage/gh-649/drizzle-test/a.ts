import type { SQLiteTable, SQLiteUpdateSetSource } from "drizzle-orm/sqlite-core";

export function f(values: SQLiteUpdateSetSource<SQLiteTable>) {
  if ("foo" in values) return values.foo;
  return undefined;
}
