import type { ReactFragment, DragContainerElement } from '__ENTRY__';
const fragment: ReactFragment = ['hello', 1];
declare const container: DragContainerElement;
const element: Element = container;
// @ts-expect-error primitive is not a drag container
const wrong: DragContainerElement = 1;
