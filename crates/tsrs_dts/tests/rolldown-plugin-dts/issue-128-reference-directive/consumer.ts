import { a, A, type JSXElements } from '__ENTRY0__';
import type { B } from '__ENTRY1__';
const tag: JSXElements = 'div';
const tags: JSXElements[] = a;
const literal: 2 = A;
const shared: B = {};
// @ts-expect-error unknown intrinsic element
const wrong: JSXElements = 'NOT_AN_ELEMENT';
