import type {
  GroupProps,
  LayoutRenderable,
  SlotProps,
  WindowProps,
  WorkspaceProps,
} from "./layout";
import type {
  TitlebarBadgeProps,
  TitlebarButtonProps,
  TitlebarGroupProps,
  TitlebarIconProps,
  TitlebarProps,
  TitlebarRenderable,
  TitlebarTextProps,
  TitlebarWindowTitleProps,
  TitlebarWorkspaceNameProps,
} from "./titlebar";

type Component<Props = Record<string, unknown>> = (props: Props) => unknown;
type JSXChild = LayoutRenderable | string | number | boolean | null | undefined;
type JSXChildren = JSXChild | JSXChild[];
type JSXPropsWithChildren<T> = T & {
  children?: JSXChildren;
};

declare global {
  const Fragment: unique symbol;
  function sp(
    type: string | typeof Fragment | Component,
    props: Record<string, unknown> | null,
    ...children: unknown[]
  ): unknown;

  namespace JSX {
    type Element = any;

    interface ElementChildrenAttribute {
      children: {};
    }

    interface IntrinsicAttributes {
      key?: string | number;
    }

    interface IntrinsicClassAttributes<T> {
      key?: string | number;
    }

    interface IntrinsicElements {
      workspace: JSXPropsWithChildren<WorkspaceProps>;
      group: JSXPropsWithChildren<GroupProps>;
      slot: JSXPropsWithChildren<SlotProps>;
      window: JSXPropsWithChildren<WindowProps>;
      titlebar: JSXPropsWithChildren<TitlebarProps>;
      "titlebar.group": JSXPropsWithChildren<TitlebarGroupProps>;
      "titlebar.windowTitle": JSXPropsWithChildren<TitlebarWindowTitleProps>;
      "titlebar.workspaceName": JSXPropsWithChildren<TitlebarWorkspaceNameProps>;
      "titlebar.text": JSXPropsWithChildren<TitlebarTextProps>;
      "titlebar.badge": JSXPropsWithChildren<TitlebarBadgeProps>;
      "titlebar.button": JSXPropsWithChildren<TitlebarButtonProps>;
      "titlebar.icon": JSXPropsWithChildren<TitlebarIconProps>;
    }

    type LibraryManagedAttributes<C, P> = JSXPropsWithChildren<P>;
  }
}

export declare function sp(
  type: string | typeof Fragment | Component,
  props: Record<string, unknown> | null,
  ...children: unknown[]
): unknown;
export declare function jsx(
  type: string | typeof Fragment | Component,
  props: Record<string, unknown> | null,
  key?: unknown,
): unknown;
export declare function jsxs(
  type: string | typeof Fragment | Component,
  props: Record<string, unknown> | null,
  key?: unknown,
): unknown;
export { Fragment };

export {};
