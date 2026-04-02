export type EditorFileId =
  | "config"
  | "config-bindings"
  | "config-inputs"
  | "config-layouts"
  | "root-css"
  | "layout-tsx"
  | "layout-css"
  | "focus-repro-layout-tsx"
  | "focus-repro-layout-css";

export interface EditorFile {
  id: EditorFileId;
  label: string;
  path: string;
  modelPath: string;
  language: "typescript" | "typescriptreact" | "css";
  initialValue: string;
  icon: string;
  iconTone: "text-file-ts" | "text-file-tsx" | "text-file-css";
}

export interface FileTreeDirectoryNode {
  kind: "directory";
  name: string;
  defaultOpen?: boolean;
  downloadRootPath?: string;
  children: FileTreeNode[];
}

export interface FileTreeFileNode {
  kind: "file";
  fileId: EditorFileId;
}

export type FileTreeNode = FileTreeDirectoryNode | FileTreeFileNode;
