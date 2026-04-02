import "./App.css";
import layoutStylesheetSource from "./layouts/master-stack/index.css?raw";
import layoutSource from "./layouts/master-stack/index.tsx?raw";
import layout from "./layouts/master-stack/index.tsx";
import {
  ALLOWED_LAYOUT_PROPERTIES,
  validateLayoutStylesheet,
} from "./layout-runtime/css-subset";
import {
  resolveLayout,
  type ResolvedLayoutNode,
} from "./layout-runtime/resolve";
import type {
  LayoutContext,
  LayoutDiagnostic,
  LayoutWindow,
} from "@spiders-wm/sdk/layout";

const mockWindows: LayoutWindow[] = [
  {
    id: "win-1",
    app_id: "foot",
    title: "Terminal 1",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
    focused: true,
  },
  {
    id: "win-2",
    app_id: "foot",
    title: "Terminal 2",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
  },
  {
    id: "win-3",
    app_id: "zen",
    title: "Spec Draft",
    class: "zen-browser",
    instance: "zen",
    shell: "xdg_toplevel",
  },
  {
    id: "win-4",
    app_id: "slack",
    title: "Engineering",
    class: "Slack",
    instance: "slack",
    shell: "xdg_toplevel",
  },
  {
    id: "win-5",
    app_id: "spotify",
    title: "Now Playing",
    class: "Spotify",
    instance: "spotify",
    shell: "xdg_toplevel",
    floating: true,
  },
];

const mockContext: LayoutContext = {
  monitor: {
    name: "DP-1",
    width: 3440,
    height: 1440,
    scale: 1,
  },
  workspace: {
    name: "1:dev",
    workspaces: ["1:dev", "2:web", "3:chat"],
    windowCount: mockWindows.length,
  },
  windows: mockWindows,
  state: {
    prototype: true,
  },
};

function WindowPills({ windows }: { windows: LayoutWindow[] }) {
  if (windows.length === 0) {
    return <p className="muted">No windows claimed.</p>;
  }

  return (
    <ul className="window-pills">
      {windows.map((window) => (
        <li key={window.id}>
          <span className="window-pill__title">
            {window.title ?? window.id}
          </span>
          <span className="window-pill__meta">
            {window.app_id ?? "unknown"}
          </span>
        </li>
      ))}
    </ul>
  );
}

function LayoutTreeNode({ node }: { node: ResolvedLayoutNode }) {
  const id = typeof node.props?.id === "string" ? node.props.id : null;
  const className =
    typeof node.props?.class === "string" ? node.props.class : null;
  const match =
    node.type === "window" &&
    node.props &&
    "match" in node.props &&
    typeof node.props.match === "string"
      ? node.props.match
      : null;
  const take =
    node.type === "slot" &&
    node.props &&
    "take" in node.props &&
    typeof node.props.take === "number"
      ? node.props.take
      : null;

  return (
    <article className={`tree-node tree-node--${node.type}`}>
      <header className="tree-node__header">
        <div>
          <p className="eyebrow">{node.type}</p>
          <h3>{id ? `#${id}` : node.type}</h3>
        </div>
        <p className="tree-node__path">{node.path}</p>
      </header>

      <dl className="tree-node__props">
        <div>
          <dt>class</dt>
          <dd>{className ?? "none"}</dd>
        </div>
        <div>
          <dt>match</dt>
          <dd>{match ?? "none"}</dd>
        </div>
        <div>
          <dt>take</dt>
          <dd>{take ?? "remaining"}</dd>
        </div>
      </dl>

      <WindowPills windows={node.claimedWindows} />

      {node.children.length > 0 ? (
        <div className="tree-node__children">
          {node.children.map((child) => (
            <LayoutTreeNode key={child.path} node={child} />
          ))}
        </div>
      ) : null}
    </article>
  );
}

function DiagnosticsPanel({
  diagnostics,
}: {
  diagnostics: LayoutDiagnostic[];
}) {
  if (diagnostics.length === 0) {
    return <p className="muted">No layout or CSS diagnostics.</p>;
  }

  return (
    <ul className="diagnostics-list">
      {diagnostics.map((diagnostic, index) => (
        <li key={`${diagnostic.source}-${diagnostic.path ?? "root"}-${index}`}>
          <span
            className={`diagnostic-pill diagnostic-pill--${diagnostic.level}`}
          >
            {diagnostic.level}
          </span>
          <div>
            <strong>{diagnostic.source}</strong>
            <p>{diagnostic.message}</p>
            {diagnostic.path ? <code>{diagnostic.path}</code> : null}
          </div>
        </li>
      ))}
    </ul>
  );
}

function App() {
  const resolvedLayout = resolveLayout(layout(mockContext), mockContext);
  const cssDiagnostics = validateLayoutStylesheet(layoutStylesheetSource);
  const diagnostics = [...resolvedLayout.diagnostics, ...cssDiagnostics];

  return (
    <div className="shell">
      <header className="hero">
        <p className="eyebrow">spiders-wm playground</p>
        <h1>Custom JSX layouts before compositor policy</h1>
        <p className="hero__copy">
          This prototype evaluates layout JSX with a playground-local runtime,
          resolves window claims against mock state, and validates layout CSS
          against the documented subset.
        </p>
      </header>

      <main className="workspace-grid">
        <section className="panel panel--preview">
          <div className="panel__header">
            <div>
              <p className="eyebrow">preview</p>
              <h2>Resolved layout tree</h2>
            </div>
            <span className="stat">{mockContext.windows.length} windows</span>
          </div>

          {resolvedLayout.root ? (
            <LayoutTreeNode node={resolvedLayout.root} />
          ) : (
            <p className="muted">No valid workspace root was produced.</p>
          )}
        </section>

        <section className="panel panel--state">
          <div className="panel__header">
            <div>
              <p className="eyebrow">runtime input</p>
              <h2>Mock layout context</h2>
            </div>
          </div>

          <div className="context-grid">
            <article>
              <h3>Monitor</h3>
              <p>{mockContext.monitor.name}</p>
              <p>
                {mockContext.monitor.width} × {mockContext.monitor.height}
              </p>
            </article>
            <article>
              <h3>Workspace</h3>
              <p>{mockContext.workspace.name}</p>
              <p>{mockContext.workspace.workspaces?.join(" · ")}</p>
            </article>
          </div>

          <div className="stack">
            <div>
              <h3>Incoming windows</h3>
              <WindowPills windows={mockContext.windows} />
            </div>
            <div>
              <h3>Unclaimed windows</h3>
              <WindowPills windows={resolvedLayout.unclaimedWindows} />
            </div>
            <div>
              <h3>Diagnostics</h3>
              <DiagnosticsPanel diagnostics={diagnostics} />
            </div>
          </div>
        </section>

        <section className="panel panel--source">
          <div className="panel__header">
            <div>
              <p className="eyebrow">layout source</p>
              <h2>master-stack JSX</h2>
            </div>
          </div>
          <pre>{layoutSource}</pre>
        </section>

        <section className="panel panel--source">
          <div className="panel__header">
            <div>
              <p className="eyebrow">stylesheet subset</p>
              <h2>Allowed layout CSS</h2>
            </div>
            <span className="stat">
              {ALLOWED_LAYOUT_PROPERTIES.length} props
            </span>
          </div>

          <p className="muted muted--spaced">
            The playground currently validates simple rules and rejects
            unsupported selectors and properties early.
          </p>
          <pre>{layoutStylesheetSource}</pre>
        </section>
      </main>
    </div>
  );
}

export default App;
