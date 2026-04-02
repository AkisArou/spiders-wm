import type {
  LayoutChild,
  LayoutContext,
  LayoutDiagnostic,
  LayoutNode,
  LayoutRenderable,
  LayoutWindow,
} from "./layout";

const supportedMatchKeys = new Set([
  "app_id",
  "title",
  "class",
  "instance",
  "role",
  "shell",
  "window_type",
]);

export interface ResolvedLayoutNode {
  type: LayoutNode["type"];
  props?: LayoutNode["props"];
  path: string;
  claimedWindows: LayoutWindow[];
  children: ResolvedLayoutNode[];
}

export interface ResolvedLayoutResult {
  root: ResolvedLayoutNode | null;
  diagnostics: LayoutDiagnostic[];
  unclaimedWindows: LayoutWindow[];
}

interface MatchClause {
  key: keyof LayoutWindow;
  value: string;
}

export function resolveLayout(
  renderable: LayoutRenderable,
  context: LayoutContext,
): ResolvedLayoutResult {
  const diagnostics: LayoutDiagnostic[] = [];
  const rootNodes = collectNodes(renderable, diagnostics, "root");

  if (rootNodes.length === 0) {
    diagnostics.push({
      source: "layout",
      level: "error",
      message: "Layout must return a workspace root node.",
      path: "root",
    });

    return {
      root: null,
      diagnostics,
      unclaimedWindows: context.windows,
    };
  }

  if (rootNodes.length > 1) {
    diagnostics.push({
      source: "layout",
      level: "error",
      message: "Layout must return exactly one root node.",
      path: "root",
    });
  }

  const root = rootNodes[0];
  if (root.type !== "workspace") {
    diagnostics.push({
      source: "layout",
      level: "error",
      message: "The root node must be <workspace>.",
      path: describeNode(root, 0),
    });
  }

  const claimedWindowIds = new Set<string>();
  const resolvedRoot = resolveNode(
    root,
    context.windows,
    claimedWindowIds,
    diagnostics,
    "",
  );

  return {
    root: resolvedRoot,
    diagnostics,
    unclaimedWindows: context.windows.filter(
      (window) => !claimedWindowIds.has(window.id),
    ),
  };
}

function resolveNode(
  node: LayoutNode,
  windows: LayoutWindow[],
  claimedWindowIds: Set<string>,
  diagnostics: LayoutDiagnostic[],
  parentPath: string,
  index = 0,
): ResolvedLayoutNode {
  const path = [parentPath, describeNode(node, index)]
    .filter(Boolean)
    .join(" / ");
  const children = collectChildren(node.children, diagnostics, path);
  const claimedWindows = claimNodeWindows(
    node,
    windows,
    claimedWindowIds,
    diagnostics,
    path,
  );

  const resolvedChildren = children.map((child, childIndex) => {
    if ((node.type === "window" || node.type === "slot") && child !== null) {
      diagnostics.push({
        source: "layout",
        level: "warning",
        message: `${node.type} nodes are treated as leaves in the playground preview.`,
        path,
      });
    }

    return resolveNode(
      child,
      windows,
      claimedWindowIds,
      diagnostics,
      path,
      childIndex,
    );
  });

  return {
    type: node.type,
    props: node.props,
    path,
    claimedWindows,
    children: resolvedChildren,
  };
}

function claimNodeWindows(
  node: LayoutNode,
  windows: LayoutWindow[],
  claimedWindowIds: Set<string>,
  diagnostics: LayoutDiagnostic[],
  path: string,
): LayoutWindow[] {
  if (node.type !== "window" && node.type !== "slot") {
    return [];
  }

  const availableWindows = windows.filter(
    (window) => !claimedWindowIds.has(window.id),
  );
  const matchClauses = parseMatchClauses(
    node.type === "window" ? node.props?.match : undefined,
    diagnostics,
    path,
  );
  const matchingWindows = availableWindows.filter((window) =>
    matchesClauses(window, matchClauses),
  );

  const selectedWindows =
    node.type === "window"
      ? matchingWindows.slice(0, 1)
      : matchingWindows.slice(
          0,
          resolveTake(
            node.props?.take,
            matchingWindows.length,
            diagnostics,
            path,
          ),
        );

  for (const window of selectedWindows) {
    claimedWindowIds.add(window.id);
  }

  return selectedWindows;
}

function resolveTake(
  take: number | undefined,
  availableCount: number,
  diagnostics: LayoutDiagnostic[],
  path: string,
) {
  if (take === undefined) {
    return availableCount;
  }

  if (!Number.isInteger(take) || take < 0) {
    diagnostics.push({
      source: "layout",
      level: "error",
      message: "slot take must be a positive integer or omitted.",
      path,
    });
    return availableCount;
  }

  return take;
}

function parseMatchClauses(
  match: string | undefined,
  diagnostics: LayoutDiagnostic[],
  path: string,
): MatchClause[] {
  if (!match) {
    return [];
  }

  const clauses: MatchClause[] = [];
  const pattern = /([a-z_]+)="([^"]*)"/g;
  let consumed = "";

  for (const result of match.matchAll(pattern)) {
    const key = result[1];
    const value = result[2];
    consumed += result[0] + " ";

    if (!supportedMatchKeys.has(key)) {
      diagnostics.push({
        source: "layout",
        level: "error",
        message: `Unsupported match key: ${key}`,
        path,
      });
      continue;
    }

    clauses.push({
      key: key as keyof LayoutWindow,
      value,
    });
  }

  if (clauses.length === 0 || consumed.trim() !== match.trim()) {
    diagnostics.push({
      source: "layout",
      level: "error",
      message: `Invalid match string: ${match}`,
      path,
    });
  }

  return clauses;
}

function matchesClauses(window: LayoutWindow, clauses: MatchClause[]) {
  if (clauses.length === 0) {
    return true;
  }

  return clauses.every(
    (clause) => String(window[clause.key] ?? "") === clause.value,
  );
}

function collectNodes(
  renderable: LayoutRenderable,
  diagnostics: LayoutDiagnostic[],
  path: string,
  out: LayoutNode[] = [],
) {
  if (renderable === null) {
    return out;
  }

  if (Array.isArray(renderable)) {
    for (const child of renderable) {
      collectNodes(child, diagnostics, path, out);
    }
    return out;
  }

  if (
    typeof renderable !== "object" ||
    renderable === null ||
    !("type" in renderable)
  ) {
    diagnostics.push({
      source: "layout",
      level: "error",
      message:
        "Only supported layout elements can be returned from layout functions.",
      path,
    });
    return out;
  }

  out.push(renderable as LayoutNode);
  return out;
}

function collectChildren(
  children: LayoutChild[] | undefined,
  diagnostics: LayoutDiagnostic[],
  path: string,
) {
  const out: LayoutNode[] = [];

  for (const child of children ?? []) {
    collectNodes(child, diagnostics, path, out);
  }

  return out;
}

function describeNode(node: LayoutNode, index: number) {
  const id = typeof node.props?.id === "string" ? `#${node.props.id}` : "";
  return `${index}:${node.type}${id}`;
}
