import type {
  EditorFile,
  EditorFileId,
  FileTreeDirectoryNode,
} from "./types.ts";

export async function downloadDirectoryNode(
  node: FileTreeDirectoryNode,
  filesById: Record<EditorFileId, EditorFile>,
  buffers: Record<EditorFileId, string>,
) {
  const rootPath = node.downloadRootPath;

  if (!rootPath) {
    return;
  }

  const downloadItems = collectDownloadItems(
    node,
    rootPath,
    filesById,
    buffers,
  );

  if (downloadItems.length === 0) {
    return;
  }

  const saved = await tryWriteDirectory(node.name, downloadItems);

  if (saved !== "unsupported") {
    return;
  }

  for (const item of downloadItems) {
    downloadTextFile(
      item.content,
      formatFallbackDownloadName(node.name, item.relativePath),
    );
  }
}

export function getDownloadButtonTitle(node: FileTreeDirectoryNode) {
  const rootPath = node.downloadRootPath;

  if (!rootPath) {
    return "Download directory";
  }

  const parentPath = rootPath.slice(0, Math.max(0, rootPath.lastIndexOf("/")));

  if (!parentPath) {
    return `Choose the parent directory so ${node.name}/ is created there. If the browser does not support folder picking here, the files will be downloaded individually instead.`;
  }

  return `Choose ${parentPath}/ so ${node.name}/ is created there and the files are copied inside it. If the browser does not support folder picking here, the files will be downloaded individually instead.`;
}

function collectDownloadItems(
  node: FileTreeDirectoryNode,
  rootPath: string,
  filesById: Record<EditorFileId, EditorFile>,
  buffers: Record<EditorFileId, string>,
) {
  const items: DownloadItem[] = [];

  for (const child of node.children) {
    if (child.kind === "file") {
      const file = filesById[child.fileId];

      if (!file) {
        continue;
      }

      items.push({
        relativePath: toRelativePath(file.path, rootPath),
        content: buffers[file.id],
      });
      continue;
    }

    items.push(...collectDownloadItems(child, rootPath, filesById, buffers));
  }

  return items;
}

function toRelativePath(path: string, rootPath: string) {
  return path.startsWith(`${rootPath}/`)
    ? path.slice(rootPath.length + 1)
    : path;
}

async function tryWriteDirectory(
  directoryName: string,
  items: DownloadItem[],
): Promise<"saved" | "cancelled" | "unsupported"> {
  const pickerWindow = window as WindowWithDirectoryPicker;

  if (!pickerWindow.showDirectoryPicker) {
    return "unsupported";
  }

  try {
    const parentDirectory = await pickerWindow.showDirectoryPicker();
    const directory = await parentDirectory.getDirectoryHandle(directoryName, {
      create: true,
    });

    for (const item of items) {
      await writeDirectoryFile(directory, item.relativePath, item.content);
    }

    return "saved";
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      return "cancelled";
    }

    return "unsupported";
  }
}

async function writeDirectoryFile(
  rootDirectory: DirectoryHandleLike,
  relativePath: string,
  content: string,
) {
  const segments = relativePath.split("/");
  const fileName = segments.pop();

  if (!fileName) {
    return;
  }

  let directory = rootDirectory;

  for (const segment of segments) {
    directory = await directory.getDirectoryHandle(segment, { create: true });
  }

  const fileHandle = await directory.getFileHandle(fileName, { create: true });
  const writable = await fileHandle.createWritable();
  await writable.write(content);
  await writable.close();
}

function downloadTextFile(content: string, fileName: string) {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  link.click();
  URL.revokeObjectURL(url);
}

function formatFallbackDownloadName(
  directoryName: string,
  relativePath: string,
) {
  return `${directoryName}__${relativePath.replaceAll("/", "__")}`;
}

type DownloadItem = {
  relativePath: string;
  content: string;
};

type WindowWithDirectoryPicker = Window & {
  showDirectoryPicker?: () => Promise<DirectoryHandleLike>;
};

type DirectoryHandleLike = {
  getDirectoryHandle: (
    name: string,
    options: { create: boolean },
  ) => Promise<DirectoryHandleLike>;
  getFileHandle: (
    name: string,
    options: { create: boolean },
  ) => Promise<FileHandleLike>;
};

type FileHandleLike = {
  createWritable: () => Promise<WritableFileLike>;
};

type WritableFileLike = {
  write: (content: string) => Promise<void>;
  close: () => Promise<void>;
};
