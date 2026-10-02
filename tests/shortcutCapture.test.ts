import assert from "node:assert/strict";
import test from "node:test";
import { shortcutFromStroke } from "../src/shortcutCapture.ts";

const stroke = (code: string, modifiers = {}) => ({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...modifiers });

test("captures all held modifiers with a physical key regardless of keyboard language", () => {
  assert.equal(shortcutFromStroke(stroke("KeyM", { ctrlKey: true, altKey: true, shiftKey: true, metaKey: true })), "Ctrl+Alt+Shift+Super+M");
  assert.equal(shortcutFromStroke(stroke("Digit2", { metaKey: true })), "Super+2");
});

test("modifier-only strokes and repeats never replace the saved shortcut", () => {
  assert.equal(shortcutFromStroke(stroke("ControlLeft", { ctrlKey: true })), null);
  assert.equal(shortcutFromStroke(stroke("KeyM", { ctrlKey: true, repeat: true })), null);
  assert.equal(shortcutFromStroke(stroke("Unidentified")), null);
});

test("captures modifiers pressed after the main key while it is still held", () => {
  assert.equal(shortcutFromStroke(stroke("ControlLeft", { ctrlKey: true }), "KeyM"), "Ctrl+M");
  assert.equal(shortcutFromStroke(stroke("AltLeft", { ctrlKey: true, altKey: true }), "KeyM"), "Ctrl+Alt+M");
});

test("supports function, arrow and punctuation keys accepted by the backend", () => {
  assert.equal(shortcutFromStroke(stroke("F12", { shiftKey: true })), "Shift+F12");
  assert.equal(shortcutFromStroke(stroke("ArrowUp", { ctrlKey: true })), "Ctrl+ArrowUp");
  assert.equal(shortcutFromStroke(stroke("Equal", { ctrlKey: true, shiftKey: true })), "Ctrl+Shift+Equal");
});

test("plain Tab and Escape remain available for navigation and leaving capture", () => {
  assert.equal(shortcutFromStroke(stroke("Tab")), null);
  assert.equal(shortcutFromStroke(stroke("Escape")), null);
  assert.equal(shortcutFromStroke(stroke("Tab", { ctrlKey: true })), "Ctrl+Tab");
});
