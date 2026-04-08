import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";

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

function ensureXtermStyles() {
  const styleId = "spiders-wm-xterm-host-css";
  if (document.getElementById(styleId)) return;

  const link = document.createElement("link");
  link.id = styleId;
  link.rel = "stylesheet";
  link.href = "/monaco/xterm-host.css";
  document.head.appendChild(link);
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

function commandFromInput(input: string) {
  const trimmed = input.trim();
  if (!trimmed) return null;

  switch (trimmed) {
    case "help":
      return { kind: "help" as const };
    case "state":
      return { kind: "query-state" as const };
    case "workspace-names":
      return { kind: "query-workspace-names" as const };
    case "subscribe":
      return { kind: "subscribe-all" as const };
    case "cycle-layout":
      return { kind: "cycle-layout" as const };
    case "clear":
      return { kind: "clear" as const };
    default:
      return { kind: "unknown" as const, input: trimmed };
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

  const handle: XtermHostHandle = {
    host,
    terminal,
    fitAddon,
    currentInput: "",
    beforeInput: "",
    prompt: "ipc> ",
    onDataDisposable: { dispose() {} },
    suggestionAnchor: document.createElement("div"),
    resizeObserver: new ResizeObserver(() => {
      fitAddon.fit();
    }),
  };

  terminal.writeln("spiders-wm browser ipc terminal");
  terminal.writeln("type: help, state, workspace-names, subscribe, cycle-layout, clear");
  terminal.write(handle.prompt);

  handle.onDataDisposable = terminal.onData((data) => {
    switch (data) {
      case "\r": {
        const input = handle.currentInput;
        const command = commandFromInput(input);
        terminal.write("\r\n");
        handle.currentInput = "";

        if (!command) {
          terminal.write(handle.prompt);
          return;
        }

        if (command.kind === "help") {
          terminal.writeln("help: show commands");
          terminal.writeln("state: query state snapshot");
          terminal.writeln("workspace-names: query configured workspaces");
          terminal.writeln("subscribe: subscribe to all IPC events");
          terminal.writeln("cycle-layout: dispatch cycle-layout command");
          terminal.writeln("clear: clear terminal output");
          terminal.write(handle.prompt);
          return;
        }

        if (command.kind === "clear") {
          terminal.clear();
          terminal.write(handle.prompt);
          return;
        }

        if (command.kind === "unknown") {
          terminal.writeln(`unknown command: ${command.input}`);
          terminal.write(handle.prompt);
          return;
        }

        onCommand(command.kind);
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
  (globalThis as any).__spidersXtermDebug = handle;
  return handle;
}

export function replaceXtermInput(handle: XtermHostHandle, input: string) {
  replaceCurrentInput(handle, input);
}

export function xtermInput(handle: XtermHostHandle) {
  return handle.currentInput;
}

export function writeXtermLines(handle: XtermHostHandle, lines: string[]) {
  if (!lines.length) return;
  for (const line of lines) {
    handle.terminal.writeln(line);
  }
  handle.terminal.write(handle.prompt + handle.currentInput);
}

export function clearXtermTerminal(handle: XtermHostHandle) {
  handle.terminal.clear();
  handle.terminal.write(handle.prompt + handle.currentInput);
}

export function disposeXtermTerminal(handle: XtermHostHandle) {
  handle.resizeObserver.disconnect();
  handle.onDataDisposable.dispose();
  handle.terminal.dispose();
}
