export interface ParsedBindingEntry {
  bind: string[];
  chord: string;
  commandName: string;
  commandArg?: string | number;
  commandLabel: string;
}

const bindingEntryPattern = /{\s*bind:\s*\[([\s\S]*?)\],\s*command:\s*([\s\S]*?)\s*},?/g;

export function parseBindingsSource(source: string): { mod: string; entries: ParsedBindingEntry[] } {
  const mod = source.match(/\bmod:\s*"([^"]+)"/)?.[1] ?? "super";
  const entries = Array.from(source.matchAll(bindingEntryPattern), (match) => {
    const bindSource = match[1] ?? "";
    const commandSource = match[2] ?? "";
    const bind = Array.from(bindSource.matchAll(/"([^"]+)"/g), ([, token]) => token ?? "").filter(Boolean);
    const parsedCommand = parseCommand(commandSource);

    return {
      bind,
      chord: bind.map((token) => formatBindingToken(token, mod)).join(" + "),
      commandName: parsedCommand.name,
      commandArg: parsedCommand.arg,
      commandLabel: formatBindingCommand(parsedCommand.name, parsedCommand.arg),
    };
  });

  return { mod, entries };
}

export function formatBindingToken(token: string, mod: string): string {
  const resolved = token === "mod" ? mod : token;

  switch (resolved) {
    case "alt":
    case "mod1":
      return "Alt";
    case "super":
    case "logo":
    case "mod4":
      return "Super";
    case "ctrl":
    case "control":
      return "Ctrl";
    case "shift":
      return "Shift";
    case "space":
      return "Space";
    case "Return":
      return "Enter";
    default:
      return resolved.length === 1 ? resolved.toUpperCase() : resolved;
  }
}

export function formatBindingCommand(commandName: string, arg?: string | number): string {
  if (!commandName) {
    return "unknown";
  }

  return arg === undefined ? commandName : `${commandName}(${String(arg)})`;
}

export function matchesBindingEvent(
  entry: ParsedBindingEntry,
  event: KeyboardEvent,
  mod: string,
): boolean {
  const keyToken = entry.bind.at(-1);

  if (!keyToken) {
    return false;
  }

  const expected = expectedModifiers(entry.bind.slice(0, -1), mod);
  const actualKey = normalizeKeyboardEventKey(event);

  if (!actualKey || normalizeBindingKey(keyToken) !== actualKey) {
    return false;
  }

  return (
    event.altKey === expected.alt &&
    event.ctrlKey === expected.ctrl &&
    event.metaKey === expected.meta &&
    event.shiftKey === expected.shift
  );
}

function parseCommand(source: string): { name: string; arg?: string | number } {
  const compact = source.replace(/\s+/g, " ").trim();
  const match = compact.match(/^commands\.([a-z_]+)\((.*)\)$/i);

  if (!match) {
    return { name: compact || "unknown" };
  }

  const name = match[1] ?? "unknown";
  const rawArg = (match[2] ?? "").trim();

  if (!rawArg) {
    return { name };
  }

  const stringMatch = rawArg.match(/^"([\s\S]*)"$/);

  if (stringMatch) {
    return { name, arg: stringMatch[1] ?? "" };
  }

  if (/^-?\d+$/.test(rawArg)) {
    return { name, arg: Number(rawArg) };
  }

  return { name, arg: rawArg };
}

function expectedModifiers(bind: string[], mod: string) {
  const resolved = bind.map((token) => resolveModifierToken(token, mod));

  return {
    alt: resolved.includes("alt"),
    ctrl: resolved.includes("ctrl"),
    meta: resolved.includes("meta"),
    shift: resolved.includes("shift"),
  };
}

function resolveModifierToken(token: string, mod: string): "alt" | "ctrl" | "meta" | "shift" | null {
  const resolved = token === "mod" ? mod : token;

  switch (resolved) {
    case "alt":
    case "mod1":
      return "alt";
    case "ctrl":
    case "control":
      return "ctrl";
    case "super":
    case "logo":
    case "mod4":
      return "meta";
    case "shift":
      return "shift";
    default:
      return null;
  }
}

function normalizeBindingKey(token: string): string {
  switch (token) {
    case "Return":
      return "return";
    case "space":
      return "space";
    case "comma":
      return "comma";
    case "period":
      return "period";
    default:
      return token.toLowerCase();
  }
}

function normalizeKeyboardEventKey(event: KeyboardEvent): string | null {
  if (event.code.startsWith("Key")) {
    return event.code.slice(3).toLowerCase();
  }

  if (event.code.startsWith("Digit")) {
    return event.code.slice(5).toLowerCase();
  }

  switch (event.code) {
    case "Enter":
      return "return";
    case "Space":
      return "space";
    case "Comma":
      return "comma";
    case "Period":
      return "period";
    case "ArrowLeft":
      return "left";
    case "ArrowRight":
      return "right";
    case "ArrowUp":
      return "up";
    case "ArrowDown":
      return "down";
    default:
      return event.key ? event.key.toLowerCase() : null;
  }
}