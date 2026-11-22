type Props = { header?: (opts?: { actions?: string }) => string };
declare function render(props: Props): void;

render({ header: ({ actions } = {}) => actions ?? "" });
