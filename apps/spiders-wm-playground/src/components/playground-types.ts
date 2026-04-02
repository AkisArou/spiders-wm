export type EditorFileId = "config" | "root-css" | "layout-tsx" | "layout-css";

export interface EditorFile {
  id: EditorFileId;
  label: string;
  path: string;
  modelPath: string;
  language: "typescript" | "css";
  initialValue: string;
  icon: string;
  iconTone: "text-file-ts" | "text-file-tsx" | "text-file-css";
}

export interface FileTreeDirectoryNode {
  kind: "directory";
  name: string;
  defaultOpen?: boolean;
  children: FileTreeNode[];
}

export interface FileTreeFileNode {
  kind: "file";
  fileId: EditorFileId;
}

export type FileTreeNode = FileTreeDirectoryNode | FileTreeFileNode;
