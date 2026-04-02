import type { LayoutWindow } from "@spiders-wm/sdk/layout";

import initBindings, {
  apply_preview_command,
  apply_preview_snapshot_overrides,
  compute_layout_preview,
} from "./generated/spiders-web-bindings/spiders_web_bindings.js";
import wasmUrl from "./generated/spiders-web-bindings/spiders_web_bindings_bg.wasm?url";

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

export interface PreviewSessionWindow extends LayoutWindow {
  workspaceName: string;
}

export interface PreviewSessionState {
  activeWorkspaceName: string;
  workspaceNames: string[];
  windows: PreviewSessionWindow[];
  masterRatioByWorkspace?: Record<string, number>;
  stackWeightsByWorkspace?: Record<string, Record<string, number>>;
}

export interface PreviewSessionCommand {
  name: string;
  arg?: string | number;
}

interface RawResolvedNode {
  type: "workspace" | "group" | "window";
  id?: string;
  class?: string[];
  window_id?: string | null;
  windowId?: string | null;
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
  windowId?: string | null;
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
    initPromise = initBindings({ module_or_path: wasmUrl }).then(
      () => undefined,
    );
  }

  return initPromise;
}

export async function computePreview(
  layoutRenderable: unknown,
  windows: LayoutWindow[],
  stylesheetSource: string,
  width: number,
  height: number,
  sessionState?: PreviewSessionState | null,
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
  const resolvedRoot = toPlainValue(raw.resolved_root) as RawResolvedNode | null;
  let snapshotRoot = toPlainValue(raw.snapshot_root) as RawSnapshotNode | null;

  if (sessionState && snapshotRoot) {
    snapshotRoot = toPlainValue(
      apply_preview_snapshot_overrides(sessionState, snapshotRoot),
    ) as RawSnapshotNode | null;
  }

  return {
    treeRoot: normalizeTree(resolvedRoot, snapshotRoot, windowsById),
    snapshotRoot: normalizeSnapshot(snapshotRoot),
    diagnostics: raw.diagnostics,
    unclaimedWindows: raw.unclaimed_windows,
  };
}

export async function applyPreviewCommand(
  state: PreviewSessionState,
  command: PreviewSessionCommand,
  snapshotRoot: PreviewSnapshotNode | null,
): Promise<PreviewSessionState> {
  await ensureBindings();

  return apply_preview_command(state, command, snapshotRoot) as PreviewSessionState;
}

function toPlainValue<T>(value: T): T {
  if (value instanceof Map) {
    return Object.fromEntries(
      Array.from(value.entries(), ([key, entry]) => [key, toPlainValue(entry)]),
    ) as T;
  }

  if (Array.isArray(value)) {
    return value.map((entry) => toPlainValue(entry)) as T;
  }

  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, toPlainValue(entry)]),
    ) as T;
  }

  return value;
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
    typeof (rawNode.window_id ?? rawNode.windowId) === "string"
      ? windowsById.get((rawNode.window_id ?? rawNode.windowId) as string) ?? null
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
    windowId:
      typeof (rawNode.window_id ?? rawNode.windowId) === "string"
        ? ((rawNode.window_id ?? rawNode.windowId) as string)
        : null,
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