type IntegrationDefinition = {
  id: string;
  icon: (props?: { className?: string; style?: Record<string, string | number> }) => unknown;
};
declare function cn(...values: (string | undefined)[]): string;
export const integrations: Array<IntegrationDefinition> = [
  {
    id: "queues",
    icon: ({ className, ...props } = {}) => [cn("icon", className), props],
  },
];
