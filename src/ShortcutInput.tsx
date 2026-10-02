import { useRef, useState, type KeyboardEvent } from "react";
import { heldModifiers, shortcutFromStroke } from "./shortcutCapture";

interface Props {
  value: string;
  ready: boolean;
  onChange: (value: string) => void;
}

export function ShortcutInput({ value, ready, onChange }: Props) {
  const [preview, setPreview] = useState<string | null>(null);
  const heldKey = useRef<string | undefined>(undefined);

  function record(event: KeyboardEvent<HTMLInputElement>) {
    if (event.code === "Tab" && !event.ctrlKey && !event.altKey && !event.metaKey) return;
    event.preventDefault();
    if (event.code === "Escape" && !heldModifiers(event).length) {
      event.currentTarget.blur();
      return;
    }
    if (!ready) return;
    const shortcut = shortcutFromStroke(event, heldKey.current);
    if (shortcut) {
      if (!/^(Control|Alt|Shift|Meta)(Left|Right)$/.test(event.code)) heldKey.current = event.code;
      onChange(shortcut);
      setPreview(null);
    } else if (!event.repeat) {
      setPreview(heldModifiers(event).join("+"));
    }
  }

  return <input
    readOnly
    data-shortcut-input="true"
    value={preview ?? value}
    placeholder={ready ? "키 조합을 누르세요" : "입력 준비 중…"}
    aria-describedby="shortcut-help"
    autoComplete="off"
    onFocus={() => { heldKey.current = undefined; setPreview(""); }}
    onBlur={() => { heldKey.current = undefined; setPreview(null); }}
    onKeyDown={record}
    onKeyUp={(event) => {
      if (heldKey.current === event.code) heldKey.current = undefined;
      setPreview(null);
    }}
    onPaste={(event) => event.preventDefault()}
    onDrop={(event) => event.preventDefault()}
  />;
}
