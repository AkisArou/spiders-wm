import type { EditorFile, EditorFileId, FileTreeNode } from "./playground-types.ts";

export function FileTree({
  node,
  filesById,
  activeFileId,
  dirtyFiles,
  onSelect,
  depth = 0,
}: {
  node: FileTreeNode;
  filesById: Record<EditorFileId, EditorFile>;
  activeFileId: EditorFileId;
  dirtyFiles: Set<EditorFileId>;
  onSelect: (fileId: EditorFileId) => void;
  depth?: number;
}) {
  if (node.kind === "file") {
    const file = filesById[node.fileId];

    return (
      <button
        type="button"
        className={[
          "flex w-full items-center gap-2 px-2 py-1 text-left text-sm leading-5",
          activeFileId === file.id
            ? "bg-terminal-bg-active text-terminal-fg-strong"
            : "text-terminal-muted hover:bg-terminal-bg-hover hover:text-terminal-fg",
        ].join(" ")}
        onClick={() => onSelect(file.id)}
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        <span className={file.iconTone}>{file.icon}</span>
        <span className="truncate">{file.label}</span>
        {dirtyFiles.has(file.id) ? <span className="ml-auto text-terminal-warn">+</span> : null}
      </button>
    );
  }

  return (
    <div>
      <div
        className="flex items-center gap-1 px-2 py-1 text-sm leading-5 text-terminal-dim"
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        <span>{node.defaultOpen === false ? "▸" : "▾"}</span>
        <span>{node.name}</span>
      </div>

      {node.children.map((child) => (
        <div key={child.kind === "file" ? child.fileId : `${node.name}/${child.name}`}>
          <FileTree
            node={child}
            filesById={filesById}
            activeFileId={activeFileId}
            dirtyFiles={dirtyFiles}
            onSelect={onSelect}
            depth={depth + 1}
          />
        </div>
      ))}
    </div>
  );
}