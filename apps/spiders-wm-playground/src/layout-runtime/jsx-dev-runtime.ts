import { Fragment, jsx, type JSX } from "./jsx-runtime";

export { Fragment };
export type { JSX };

export function jsxDEV(
  type: string | typeof Fragment,
  props: Record<string, unknown> | null,
  key?: unknown,
) {
  return jsx(type, props, key);
}
