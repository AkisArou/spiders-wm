import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";

const xtermCssHref = new URL("@xterm/xterm/css/xterm.css", import.meta.url).href;

interface XtermHostHandle {
  host: HTMLElement;
  terminal: Terminal;
  fitAddon: FitAddon;
  onDataDisposable: { dispose(): void };
  resizeObserver: ResizeObserver;
  currentInput: string;
  prompt: string;
  beforeInput: string;
  suggestionAnchor: HTMLDivElement;
}

const PROMPT = "cli> ";

function ensureXtermStyles() {
  const styleId = "spiders-wm-xterm-host-css";
  if (document.getElementById(styleId)) return;

  const link = document.createElement("link");
  link.id = styleId;
  link.rel = "stylesheet";
  link.href = xtermCssHref;
  document.head.appendChild(link);
}

function hideHelperTextarea(host: HTMLElement) {
  const helper = host.querySelector<HTMLTextAreaElement>(".xterm-helper-textarea");
  if (!helper) return;

  helper.setAttribute("aria-label", "Terminal input");
  helper.style.position = "absolute";
  helper.style.left = "-9999px";
  helper.style.top = "0";
  helper.style.width = "1px";
  helper.style.height = "1px";
  helper.style.opacity = "0";
}

function printPrompt(handle: XtermHostHandle) {
  handle.terminal.write(`\r\n${handle.prompt}`);
}

function replaceCurrentInput(handle: XtermHostHandle, nextInput: string) {
  const previousWidth = handle.currentInput.length;
  handle.currentInput = nextInput;
  handle.terminal.write(`\r${handle.prompt}${nextInput}`);
  if (previousWidth > nextInput.length) {
    handle.terminal.write(" ".repeat(previousWidth - nextInput.length));
    handle.terminal.write(`\r${handle.prompt}${nextInput}`);
  }
}

export function createXtermTerminal(
  host: HTMLElement,
  onCommand: (command: string) => void,
  onTabComplete?: (input: string) => void,
) {
  ensureXtermStyles();

  const terminal = new Terminal({
    allowTransparency: false,
    convertEol: true,
    cursorBlink: true,
    cursorStyle: "block",
    disableStdin: false,
    fontFamily:
      '"JetBrainsMono Nerd Font", "Symbols Nerd Font Mono", "IBM Plex Mono", monospace',
    fontSize: 13,
    lineHeight: 1.45,
    rows: 24,
    theme: {
      background: "#1b1f24",
      foreground: "#d6deeb",
      cursor: "#d6deeb",
      black: "#1b1f24",
      brightBlack: "#5c6370",
      red: "#ef5350",
      green: "#8bc34a",
      yellow: "#fbc02d",
      blue: "#61afef",
      magenta: "#c678dd",
      cyan: "#56b6c2",
      white: "#d6deeb",
      brightWhite: "#ffffff",
    },
  });
  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  terminal.open(host);
  fitAddon.fit();
  hideHelperTextarea(host);
  host.addEventListener("mousedown", () => {
    terminal.focus();
    hideHelperTextarea(host);
  });

  const handle: XtermHostHandle = {
    host,
    terminal,
    fitAddon,
    currentInput: "",
    beforeInput: "",
    prompt: PROMPT,
    onDataDisposable: { dispose() {} },
    suggestionAnchor: document.createElement("div"),
    resizeObserver: new ResizeObserver(() => {
      fitAddon.fit();
      hideHelperTextarea(host);
    }),
  };

  handle.onDataDisposable = terminal.onData((data) => {
    switch (data) {
      case "\r": {
        const input = handle.currentInput;
        terminal.write("\r\n");
        handle.currentInput = "";

        if (!input.trim()) {
          terminal.write(handle.prompt);
          return;
        }

        onCommand(input);
        terminal.write(handle.prompt);
        return;
      }
      case "\t": {
        if (onTabComplete) {
          onTabComplete(handle.currentInput);
        }
        return;
      }
      case "\u007F": {
        if (!handle.currentInput.length) return;
        replaceCurrentInput(handle, handle.currentInput.slice(0, -1));
        return;
      }
      default: {
        if (data >= " " && data !== "\u007F") {
          replaceCurrentInput(handle, `${handle.currentInput}${data}`);
        }
      }
    }
  });

  handle.resizeObserver.observe(host);
  terminal.focus();
  (globalThis as any).__spidersXtermDebug = handle;
  return handle;
}

export function replaceXtermInput(handle: XtermHostHandle, input: string) {
  replaceCurrentInput(handle, input);
}

export function focusXtermTerminal(handle: XtermHostHandle) {
  handle.terminal.focus();
  hideHelperTextarea(handle.host);
}

export function xtermInput(handle: XtermHostHandle) {
  return handle.currentInput;
}

export function writeXtermLines(handle: XtermHostHandle, lines: string[]) {
  if (lines.length) {
    for (const line of lines) {
      handle.terminal.writeln(line);
    }
  }
  handle.terminal.write(handle.currentInput ? handle.prompt + handle.currentInput : handle.prompt);
}

export function clearXtermTerminal(handle: XtermHostHandle) {
  handle.terminal.clear();
}

export function disposeXtermTerminal(handle: XtermHostHandle) {
  handle.resizeObserver.disconnect();
  handle.onDataDisposable.dispose();
  handle.terminal.dispose();
}
