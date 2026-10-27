import type { DragContainerElement } from '__ENTRY__';
declare const value: DragContainerElement;
const element: Element = value;
// @ts-expect-error primitive is not a container
const wrong: DragContainerElement = 1;
