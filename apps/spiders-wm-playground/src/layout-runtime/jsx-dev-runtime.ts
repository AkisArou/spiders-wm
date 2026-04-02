import { Fragment, jsx, type JSX } from "./jsx-runtime.js";

export { Fragment };
export type { JSX };

type Component = (props: Record<string, unknown>) => unknown;

export function jsxDEV(
  type: string | typeof Fragment | Component,
  props: Record<string, unknown> | null,
  key?: unknown,
) {
  return jsx(type, props, key);
}
