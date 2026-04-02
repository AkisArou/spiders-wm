import { cn } from "../../utils/cn.ts";

import { getDownloadButtonTitle } from "./download.ts";
import type {
  EditorFile,
  EditorFileId,
  FileTreeDirectoryNode,
  FileTreeNode,
} from "./types.ts";

export function FileTree({
  node,
  filesById,
  activeFileId,
  dirtyFiles,
  onSelect,
  onDownloadDirectory,
  depth = 0,
}: {
  node: FileTreeNode;
  filesById: Record<EditorFileId, EditorFile>;
  activeFileId: EditorFileId | null;
  dirtyFiles: Set<EditorFileId>;
  onSelect: (fileId: EditorFileId) => void;
  onDownloadDirectory?: (node: FileTreeDirectoryNode) => void;
  depth?: number;
}) {
  if (node.kind === "file") {
    const file = filesById[node.fileId];

    return (
      <button
        type="button"
        className={cn(
          "flex w-full items-center gap-2 px-2 py-1 text-left text-sm leading-5",
          activeFileId === file.id
            ? "bg-terminal-bg-hover text-terminal-fg-strong"
            : "text-terminal-muted hover:bg-terminal-bg-hover hover:text-terminal-fg",
        )}
        onClick={() => onSelect(file.id)}
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        <span className={file.iconTone}>{file.icon}</span>
        <span className="truncate">{file.label}</span>
        {dirtyFiles.has(file.id) ? (
          <span className="text-terminal-warn ml-auto">+</span>
        ) : null}
      </button>
    );
  }

  return (
    <div>
      <div
        className="text-terminal-dim group flex items-center gap-1 px-2 py-1 text-sm leading-5"
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        <span>{node.defaultOpen === false ? "▸" : "▾"}</span>
        <span className="truncate">{node.name}</span>
        {node.downloadRootPath && onDownloadDirectory ? (
          <button
            type="button"
            className="border-terminal-border bg-terminal-bg-panel text-terminal-dim hover:bg-terminal-bg-hover hover:text-terminal-fg ml-auto border px-1.5 py-0 text-[11px] opacity-0 transition-opacity group-hover:opacity-100"
            title={getDownloadButtonTitle(node)}
            onClick={(event) => {
              event.stopPropagation();
              onDownloadDirectory(node);
            }}
          >
            download
          </button>
        ) : null}
      </div>

      {node.children.map((child) => (
        <div
          key={
            child.kind === "file" ? child.fileId : `${node.name}/${child.name}`
          }
        >
          <FileTree
            node={child}
            filesById={filesById}
            activeFileId={activeFileId}
            dirtyFiles={dirtyFiles}
            onSelect={onSelect}
            onDownloadDirectory={onDownloadDirectory}
            depth={depth + 1}
          />
        </div>
      ))}
    </div>
  );
}
