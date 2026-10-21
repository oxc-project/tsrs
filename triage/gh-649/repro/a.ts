type Update<T, TUpdate> = { [K in Exclude<keyof T, keyof TUpdate>]: T[K] } & TUpdate;
type Simplify<T> = { [K in keyof T]: T[K] } & {};
interface ColumnBaseConfig {
    data: unknown;
    notNull: boolean;
}
type ColumnTypeConfig<T extends ColumnBaseConfig, TTypeConfig extends object> = T & {} & TTypeConfig;
declare abstract class Column<T extends ColumnBaseConfig = ColumnBaseConfig, TTypeConfig extends object = object> {
    readonly table: Table;
    readonly _: ColumnTypeConfig<T, TTypeConfig>;
}
type AnyColumn<TPartial extends Partial<ColumnBaseConfig> = {}> = Column<Required<Update<ColumnBaseConfig, TPartial>>>;
type GetColumnData<TColumn extends Column> = TColumn['_']['notNull'] extends true ? TColumn['_']['data'] : TColumn['_']['data'] | null;
type RequiredKeyOnly<TKey extends string, T extends Column> = T extends AnyColumn<{ notNull: true; hasDefault: false }> ? TKey : never;
type OptionalKeyOnly<TKey extends string, T extends Column> = TKey extends RequiredKeyOnly<TKey, T> ? never
    : T extends { _: { generated: undefined } } ? (T extends { _: { identity: undefined } } ? TKey : never) : never;
interface TableConfig<TColumn extends Column = Column<any>> {
    columns: Record<string, TColumn>;
}
declare class Table<T extends TableConfig = TableConfig> {
    readonly _: {
        readonly columns: T['columns'];
    };
    readonly $inferInsert: InferInsertModel<Table<T>>;
}
type InferModelFromColumns<TColumns extends Record<string, Column>> = Simplify<{
    [Key in keyof TColumns & string as RequiredKeyOnly<Key, TColumns[Key]>]: GetColumnData<TColumns[Key]>;
} & {
    [Key in keyof TColumns & string as OptionalKeyOnly<Key, TColumns[Key]>]?: GetColumnData<TColumns[Key]> | undefined;
}>;
type InferInsertModel<TTable extends Table> = InferModelFromColumns<TTable['_']['columns']>;
type SQLiteTableConfig = TableConfig<SQLiteColumn<any>>;
declare class SQLiteTable<T extends SQLiteTableConfig = SQLiteTableConfig> extends Table<T> {}
declare abstract class SQLiteColumn<T extends ColumnBaseConfig = ColumnBaseConfig, TTypeConfig extends object = {}>
    extends Column<T, TTypeConfig & { dialect: 'sqlite' }> {
    readonly table: SQLiteTable;
}
export const a: [SQLiteTable["$inferInsert"]] = 0;
