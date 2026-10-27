interface State { live: boolean } export function used() { return { live: true } as State; } export function unused() { return 'DEAD_MARKER'; }
