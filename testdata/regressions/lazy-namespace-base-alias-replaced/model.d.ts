import * as ns from './barrel';
import * as aliases from './aliases';
type Base = typeof ns;
type Aliases = typeof aliases;
interface Other { aliasToken?: string }
interface Inner<T> extends Aliases { b?: T }
export interface Two<T> extends Base, Other { a: T }
export interface Weak<T> extends Inner<T>, Other { a?: T }
