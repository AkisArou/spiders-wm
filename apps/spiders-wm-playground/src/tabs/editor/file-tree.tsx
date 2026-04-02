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
    <div className={node.downloadRootPath ? "group/layout-subtree" : undefined}>
      <div
        className="text-terminal-dim flex w-full items-center gap-1 px-2 py-1 text-sm leading-5"
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        <span>{node.defaultOpen === false ? "▸" : "▾"}</span>
        <span className="min-w-0 flex-1 truncate">{node.name}</span>
        {node.downloadRootPath && onDownloadDirectory ? (
          <DownloadDirectoryControl
            node={node}
            onDownloadDirectory={onDownloadDirectory}
            revealClassName="ml-auto opacity-0 transition-opacity group-hover/layout-subtree:opacity-100 group-focus-within/layout-subtree:opacity-100"
          />
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

function DownloadDirectoryControl({
  node,
  onDownloadDirectory,
  revealClassName,
}: {
  node: FileTreeDirectoryNode;
  onDownloadDirectory: (node: FileTreeDirectoryNode) => void;
  revealClassName?: string;
}) {
  return (
    <div className="group/download relative flex items-center">
      <button
        type="button"
        aria-label={getDownloadButtonTitle(node)}
        className={cn(
          "border-terminal-border bg-terminal-bg-panel text-terminal-dim hover:bg-terminal-bg-hover hover:text-terminal-fg border px-1.5 py-0 text-[11px]",
          revealClassName,
        )}
        onClick={(event) => {
          event.stopPropagation();
          onDownloadDirectory(node);
        }}
      >
        download
      </button>
      <div className="border-terminal-border bg-terminal-bg-panel text-terminal-fg pointer-events-none absolute top-full right-0 z-10 mt-1 hidden w-56 max-w-[calc(100vw-2rem)] border px-2 py-1 text-[11px] leading-4 wrap-break-word shadow-[0_14px_40px_rgba(0,0,0,0.45)] group-focus-within/download:block group-hover/download:block">
        {getDownloadButtonTitle(node)}
      </div>
    </div>
  );
}
