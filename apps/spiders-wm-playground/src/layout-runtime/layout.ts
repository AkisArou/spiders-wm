export type {
  GroupNode,
  GroupProps,
  LayoutBaseProps,
  LayoutChild,
  LayoutChildren,
  LayoutComponentProps,
  LayoutContext,
  LayoutFn,
  LayoutNode,
  LayoutRenderable,
  LayoutWindow,
  SlotNode,
  SlotProps,
  WindowNode,
  WindowProps,
  WorkspaceNode,
  WorkspaceProps,
} from "@spiders-wm/sdk/layout";

export interface LayoutDiagnostic {
  source: "layout" | "css";
  level: "error" | "warning";
  message: string;
  path?: string;
}
