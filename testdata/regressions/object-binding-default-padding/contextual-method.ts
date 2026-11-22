interface NavigateOptions { scroll?: boolean }
interface Router {
  push(href: string, options?: NavigateOptions): void;
  replace(href: string, options?: NavigateOptions): void;
}
declare function navigate(href: string, options: { scroll?: boolean }): void;
export function adapt(): Router {
  return {
    push(href, { scroll } = {}) {
      navigate(href, { scroll });
    },
    replace(href, { scroll } = {}) {
      navigate(href, { scroll });
    },
  };
}
