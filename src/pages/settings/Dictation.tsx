import { ClipboardPaste, Keyboard, Mic, MousePointerClick, PenLine, Space, Type } from "lucide-react";
import type { AudioDevice, Settings } from "../../types";
import { Segmented, Switch } from "../../components/controls";
import { Card, Row } from "../../components/layout";
import { SearchSelect } from "../../components/SearchSelect";
import { ShortcutInput } from "../../components/ShortcutInput";
import type { Update } from "./shared";

export function DictationPage({ settings, update, mics }: { settings: Settings; update: Update; mics: AudioDevice[] }) {
  return (
    <>
      <Card title="Start and stop" icon={<Keyboard size={16} />}>
        <Row label="Shortcut" icon={<Keyboard size={16} />} hint="Works in any app.">
          <ShortcutInput label="Dictation shortcut" value={settings.shortcut} defaultValue="CommandOrControl+Alt+Space" onChange={(v) => update((d) => void (d.shortcut = v))} />
        </Row>
        <Row label="Shortcut behavior" icon={<MousePointerClick size={16} />}>
          <Segmented
            label="Shortcut behavior"
            value={settings.mode}
            options={[
              { value: "hold", label: "Hold to talk" },
              { value: "toggle", label: "Press to start / stop" },
            ]}
            onChange={(v) => update((d) => void (d.mode = v))}
          />
        </Row>
        <Row label="Microphone" icon={<Mic size={16} />}>
          <SearchSelect
            label="Microphone"
            placeholder="Search microphones…"
            value={settings.microphone ?? ""}
            options={[{ value: "", label: "System default" }, ...mics.map((m) => ({ value: m.id, label: `${m.name}${m.isDefault ? " (default)" : ""}` }))]}
            onChange={(v) => update((d) => void (d.microphone = v || null))}
          />
        </Row>
      </Card>
      <Card title="Inserting the text" icon={<Type size={16} />}>
        <Row
          label="Review before inserting"
          icon={<PenLine size={16} />}
          hint="Edit the text in the overlay, then Ctrl+Enter inserts it."
          info="Off: words are typed live as you speak."
          htmlFor="sw-review"
        >
          <Switch id="sw-review" label="Review before inserting" checked={settings.reviewBeforeInsert} onChange={(v) => update((d) => void (d.reviewBeforeInsert = v))} />
        </Row>
        <Row label="Insert by" icon={<Type size={16} />}>
          <Segmented
            label="Insert by"
            value={settings.insertMethod}
            options={[
              { value: "type", label: "Typing", icon: <Keyboard size={13} /> },
              { value: "paste", label: "Pasting", icon: <ClipboardPaste size={13} /> },
            ]}
            onChange={(v) => update((d) => void (d.insertMethod = v))}
          />
        </Row>
        <Row label="Add a space after the text" icon={<Space size={16} />} htmlFor="sw-space">
          <Switch id="sw-space" label="Add a space after the text" checked={settings.addTrailingSpace} onChange={(v) => update((d) => void (d.addTrailingSpace = v))} />
        </Row>
      </Card>
    </>
  );
}
