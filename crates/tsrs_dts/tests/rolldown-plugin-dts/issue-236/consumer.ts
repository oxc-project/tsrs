import { Button } from '__ENTRY__';
import type { ComponentProps } from 'react';
const props: ComponentProps<typeof Button> = { onClick(event) { event.currentTarget.disabled = true; } };
// @ts-expect-error callback required
const wrong: ComponentProps<typeof Button> = {};
