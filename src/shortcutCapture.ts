export interface ShortcutStroke {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  repeat?: boolean;
  isComposing?: boolean;
}

export function heldModifiers(event: ShortcutStroke): string[] {
  return [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super"].filter((part): part is string => Boolean(part));
}

const namedKeys = new Set(["Space", "Tab", "Enter", "Escape", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Backquote", "Backslash", "BracketLeft", "BracketRight", "Comma", "Equal", "Minus", "Period", "Quote", "Semicolon", "Slash"]);

export function shortcutFromStroke(event: ShortcutStroke, heldKey?: string): string | null {
  if (event.repeat) return null;
  const modifiers = heldModifiers(event);
  if (!modifiers.length && (event.code === "Tab" || event.code === "Escape")) return null;
  let key = /^(Control|Alt|Shift|Meta)(Left|Right)$/.test(event.code) ? heldKey ?? event.code : event.code;
  if (/^Key[A-Z]$/.test(key)) key = key.slice(3);
  else if (/^Digit[0-9]$/.test(key)) key = key.slice(5);
  else if (!/^F([1-9]|1[0-9]|2[0-4])$/.test(key) && !namedKeys.has(key)) return null;
  return [...modifiers, key].join("+");
}

export function formatShortcut(value: string): string {
  const mac = navigator.platform.toLowerCase().includes("mac");
  return value.split("+").map((key) => key === "Super" ? (mac ? "Command" : "Win") : key === "Alt" && mac ? "Option" : key).join(" + ");
}
