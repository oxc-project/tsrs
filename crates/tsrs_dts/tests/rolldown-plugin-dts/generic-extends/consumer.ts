import type { AnimatedComponent } from '__ENTRY__';
declare const component: AnimatedComponent<'button'>;
component({ disabled: true });
// @ts-expect-error button disabled is boolean
component({ disabled: 'yes' });
