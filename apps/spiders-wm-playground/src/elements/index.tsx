import type {
  GroupNode,
  GroupProps,
  LayoutChild,
  LayoutChildren,
  SlotNode,
  SlotProps,
  WindowNode,
  WindowProps,
  WorkspaceNode,
  WorkspaceProps,
} from "../layout-runtime/layout.js";

function normalizeChildren(
  children: LayoutChildren | undefined,
): LayoutChild[] | undefined {
  if (children === undefined || children === null) {
    return undefined;
  }

  const flattened: unknown[] = [];

  const visit = (value: LayoutChildren | LayoutChild) => {
    if (Array.isArray(value)) {
      for (const child of value) {
        visit(child);
      }
      return;
    }

    flattened.push(value);
  };

  visit(children);

  const normalized = flattened.filter((child): child is LayoutChild => {
    if (child === null) {
      return true;
    }

    return typeof child === "object" && child !== null && "type" in child;
  });

  return normalized.length > 0 ? normalized : undefined;
}

function splitProps<T extends { children?: LayoutChildren }>(props: T) {
  const { children, ...rest } = props;
  return {
    props: rest,
    children: normalizeChildren(children),
  };
}

export function Workspace(props: WorkspaceProps = {}): WorkspaceNode {
  const next = splitProps(props);
  return {
    type: "workspace",
    props: next.props,
    children: next.children,
  };
}

export function Group(props: GroupProps = {}): GroupNode {
  const next = splitProps(props);
  return {
    type: "group",
    props: next.props,
    children: next.children,
  };
}

export function Window(props: WindowProps = {}): WindowNode {
  const next = splitProps(props);
  return {
    type: "window",
    props: next.props,
    children: next.children,
  };
}

export function Slot(props: SlotProps = {}): SlotNode {
  const next = splitProps(props);
  return {
    type: "slot",
    props: next.props,
    children: next.children,
  };
}
