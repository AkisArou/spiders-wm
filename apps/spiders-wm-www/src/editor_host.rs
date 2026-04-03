use js_sys::Promise;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryDownloadItem {
    pub relative_path: String,
    pub content: String,
}

#[wasm_bindgen(inline_js = r#"
export async function copyTextToClipboard(text) {
    if (navigator.clipboard?.write && typeof ClipboardItem !== "undefined") {
        await navigator.clipboard.write([
            new ClipboardItem({
                "text/plain": new Blob([text], { type: "text/plain" }),
            }),
        ]);
        return;
    }

    if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        return;
    }

    throw new Error("Clipboard API is unavailable in this browser context.");
}

async function writeDirectoryFile(rootDirectory, relativePath, content) {
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

function downloadTextFile(content, fileName) {
    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName;
    link.click();
    URL.revokeObjectURL(url);
}

function formatFallbackDownloadName(directoryName, relativePath) {
    return `${directoryName}__${relativePath.replaceAll("/", "__")}`;
}

export async function downloadDirectory(directoryName, items) {
    const downloadItems = Array.isArray(items) ? items : [];

    if (downloadItems.length === 0) {
        return;
    }

    if (typeof window.showDirectoryPicker === "function") {
        try {
            const parentDirectory = await window.showDirectoryPicker();
            const directory = await parentDirectory.getDirectoryHandle(directoryName, {
                create: true,
            });

            for (const item of downloadItems) {
                await writeDirectoryFile(directory, item.relativePath, item.content);
            }

            return;
        } catch (error) {
            if (error instanceof DOMException && error.name === "AbortError") {
                return;
            }
        }
    }

    for (const item of downloadItems) {
        downloadTextFile(
            item.content,
            formatFallbackDownloadName(directoryName, item.relativePath),
        );
    }
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = copyTextToClipboard)]
    fn copy_text_to_clipboard_js(text: &str) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_name = downloadDirectory)]
    fn download_directory_js(directory_name: &str, items: JsValue) -> Result<Promise, JsValue>;
}

pub async fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    let promise = copy_text_to_clipboard_js(text).map_err(js_error_message)?;
    JsFuture::from(promise).await.map_err(js_error_message)?;
    Ok(())
}

pub async fn download_directory(
    directory_name: &str,
    items: &[DirectoryDownloadItem],
) -> Result<(), String> {
    let items = serde_wasm_bindgen::to_value(items).map_err(|error| error.to_string())?;
    let promise = download_directory_js(directory_name, items).map_err(js_error_message)?;
    JsFuture::from(promise).await.map_err(js_error_message)?;
    Ok(())
}

fn js_error_message(error: JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| "clipboard access failed".to_string())
}
