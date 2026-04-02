import type {
  GroupProps,
  LayoutRenderable,
  SlotProps,
  WindowProps,
  WorkspaceProps,
} from "./layout";

type JSXChild = LayoutRenderable | string | number | boolean | null | undefined;
type JSXChildren = JSXChild | JSXChild[];
type JSXPropsWithChildren<T> = T & {
  children?: JSXChildren;
};

type RuntimeProps = Record<string, unknown> | null | undefined;

const flattenChildren = (input: unknown[], out: unknown[]) => {
  for (const child of input) {
    if (Array.isArray(child)) {
      flattenChildren(child, out);
      continue;
    }

    if (child === false || child === null || child === undefined) {
      continue;
    }

    out.push(child);
  }
};

export const Fragment = Symbol("spiders-wm-playground.fragment");

function createNode(
  type:
    | string
    | typeof Fragment
    | ((props: Record<string, unknown>) => unknown),
  props: RuntimeProps,
  ...children: unknown[]
) {
  const normalizedChildren: unknown[] = [];
  const nextProps = props ?? {};

  flattenChildren(children, normalizedChildren);

  if (type === Fragment) {
    return normalizedChildren;
  }

  if (typeof type === "function") {
    return type({
      ...nextProps,
      children: normalizedChildren,
    });
  }

  return {
    type,
    props: nextProps,
    children: normalizedChildren,
  };
}

export function jsx(
  type: string | typeof Fragment,
  props: RuntimeProps,
  key?: unknown,
) {
  const nextProps = props ?? {};
  const children = Object.prototype.hasOwnProperty.call(nextProps, "children")
    ? [nextProps.children]
    : [];
  const runtimeProps = { ...nextProps };

  void key;
  delete runtimeProps.children;

  return createNode(type, runtimeProps, ...children);
}

export function jsxs(
  type: string | typeof Fragment,
  props: RuntimeProps,
  key?: unknown,
) {
  return jsx(type, props, key);
}

export namespace JSX {
  export type Element = LayoutRenderable;

  export interface ElementChildrenAttribute {
    children: {};
  }

  export interface IntrinsicAttributes {
    key?: string | number;
  }

  export interface IntrinsicElements {
    workspace: JSXPropsWithChildren<WorkspaceProps>;
    group: JSXPropsWithChildren<GroupProps>;
    slot: JSXPropsWithChildren<SlotProps>;
    window: JSXPropsWithChildren<WindowProps>;
  }

  export type LibraryManagedAttributes<Component, Props> =
    Component extends unknown ? JSXPropsWithChildren<Props> : never;
}
