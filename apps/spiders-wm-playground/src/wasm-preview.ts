import type { LayoutWindow } from "@spiders-wm/sdk/layout";

import initBindings, {
  compute_layout_preview,
} from "./generated/spiders-web-bindings/spiders_web_bindings.js";

export interface PreviewDiagnostic {
  source: "layout" | "css";
  level: "error" | "warning";
  message: string;
  path?: string | null;
}

export interface PreviewRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PreviewTreeNode {
  type: "workspace" | "group" | "window";
  id: string | null;
  className: string | null;
  path: string;
  rect: PreviewRect | null;
  claimedWindows: LayoutWindow[];
  children: PreviewTreeNode[];
}

export interface PreviewSnapshotNode {
  type: "workspace" | "group" | "window";
  id: string | null;
  className: string | null;
  rect: PreviewRect;
  windowId: string | null;
  children: PreviewSnapshotNode[];
}

export interface PreviewComputation {
  treeRoot: PreviewTreeNode | null;
  snapshotRoot: PreviewSnapshotNode | null;
  diagnostics: PreviewDiagnostic[];
  unclaimedWindows: LayoutWindow[];
}

interface RawResolvedNode {
  type: "workspace" | "group" | "window";
  id?: string;
  class?: string[];
  window_id?: string | null;
  children?: RawResolvedNode[];
}

interface RawWrappedResolvedNode {
  workspace?: Omit<RawResolvedNode, "type">;
  group?: Omit<RawResolvedNode, "type">;
  window?: Omit<RawResolvedNode, "type">;
}

interface RawSnapshotNode {
  type: "workspace" | "group" | "window";
  id?: string;
  class?: string[];
  rect: PreviewRect;
  window_id?: string | null;
  children?: RawSnapshotNode[];
}

interface RawWrappedSnapshotNode {
  workspace?: Omit<RawSnapshotNode, "type">;
  group?: Omit<RawSnapshotNode, "type">;
  window?: Omit<RawSnapshotNode, "type">;
}

interface RawComputePreviewResult {
  resolved_root: RawResolvedNode | null;
  snapshot_root: RawSnapshotNode | null;
  diagnostics: PreviewDiagnostic[];
  unclaimed_windows: LayoutWindow[];
}

let initPromise: Promise<void> | null = null;

function ensureBindings() {
  if (!initPromise) {
    initPromise = initBindings().then(() => undefined);
  }

  return initPromise;
}

export async function computePreview(
  layoutRenderable: unknown,
  windows: LayoutWindow[],
  stylesheetSource: string,
  width: number,
  height: number,
): Promise<PreviewComputation> {
  await ensureBindings();

  const raw = compute_layout_preview(
    layoutRenderable,
    windows,
    stylesheetSource,
    width,
    height,
  ) as RawComputePreviewResult;
  const windowsById = new Map(windows.map((window) => [window.id, window]));

  return {
    treeRoot: normalizeTree(raw.resolved_root, raw.snapshot_root, windowsById),
    snapshotRoot: normalizeSnapshot(raw.snapshot_root),
    diagnostics: raw.diagnostics,
    unclaimedWindows: raw.unclaimed_windows,
  };
}

function normalizeTree(
  node: RawResolvedNode | null,
  snapshot: RawSnapshotNode | null,
  windowsById: Map<string, LayoutWindow>,
  parentPath = "",
  index = 0,
): PreviewTreeNode | null {
  const rawNode = unwrapResolvedNode(node);
  const rawSnapshot = unwrapSnapshotNode(snapshot);

  if (!rawNode) {
    return null;
  }

  const id = typeof rawNode.id === "string" ? rawNode.id : null;
  const pathSegment = `${index}:${rawNode.type}${id ? `#${id}` : ""}`;
  const path = [parentPath, pathSegment].filter(Boolean).join(" / ");
  const children = (rawNode.children ?? [])
    .map((child, childIndex) =>
      normalizeTree(
        child,
        rawSnapshot?.children?.[childIndex] ?? null,
        windowsById,
        path,
        childIndex,
      ),
    )
    .filter((child): child is PreviewTreeNode => child !== null);

  const ownWindow =
    typeof rawNode.window_id === "string"
      ? windowsById.get(rawNode.window_id) ?? null
      : null;
  const claimedWindows = ownWindow
    ? [ownWindow]
    : children.flatMap((child) => child.claimedWindows);

  return {
    type: rawNode.type,
    id,
    className: rawNode.class?.join(" ") ?? null,
    path,
    rect: rawSnapshot?.rect ?? null,
    claimedWindows,
    children,
  };
}

function unwrapResolvedNode(node: RawResolvedNode | null): RawResolvedNode | null {
  if (!node) {
    return null;
  }

  if ("type" in node) {
    return node;
  }

  const wrappedNode = node as RawResolvedNode & RawWrappedResolvedNode;

  if (wrappedNode.workspace) {
    return {
      type: "workspace",
      ...wrappedNode.workspace,
    };
  }

  if (wrappedNode.group) {
    return {
      type: "group",
      ...wrappedNode.group,
    };
  }

  if (wrappedNode.window) {
    return {
      type: "window",
      ...wrappedNode.window,
    };
  }

  return null;
}

function normalizeSnapshot(node: RawSnapshotNode | null): PreviewSnapshotNode | null {
  const rawNode = unwrapSnapshotNode(node);

  if (!rawNode || !rawNode.rect) {
    return null;
  }

  return {
    type: rawNode.type,
    id: typeof rawNode.id === "string" ? rawNode.id : null,
    className: rawNode.class?.join(" ") ?? null,
    rect: rawNode.rect,
    windowId: typeof rawNode.window_id === "string" ? rawNode.window_id : null,
    children: (rawNode.children ?? [])
      .map((child) => normalizeSnapshot(child))
      .filter((child): child is PreviewSnapshotNode => child !== null),
  };
}

function unwrapSnapshotNode(node: RawSnapshotNode | null): RawSnapshotNode | null {
  if (!node) {
    return null;
  }

  if ("type" in node && "rect" in node) {
    return node;
  }

  const wrappedNode = node as RawSnapshotNode & RawWrappedSnapshotNode;

  if (wrappedNode.workspace) {
    return {
      type: "workspace",
      ...wrappedNode.workspace,
    };
  }

  if (wrappedNode.group) {
    return {
      type: "group",
      ...wrappedNode.group,
    };
  }

  if (wrappedNode.window) {
    return {
      type: "window",
      ...wrappedNode.window,
    };
  }

  return null;
}