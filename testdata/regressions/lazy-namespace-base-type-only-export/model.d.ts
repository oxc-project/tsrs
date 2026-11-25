import * as ns from './barrel';
type Base = typeof ns;
interface Other { hiddenToken: string }
export interface Box<T> extends Base { a: T }
export interface Two<T> extends Base, Other { a: T }
