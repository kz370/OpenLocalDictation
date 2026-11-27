import { Check, FileText, FolderOpen, Monitor, Moon, Palette, Power, SlidersHorizontal, Sun } from "lucide-react";
import { ipc } from "../../ipc";
import { useProfiles } from "../../profiles";
import type { AppInfo, Settings } from "../../types";
import { Segmented, Switch } from "../../components/controls";
import { Card, Row } from "../../components/layout";
import type { Update } from "./shared";

export function AppPage({ settings, update, info }: { settings: Settings; update: Update; info: AppInfo | null }) {
  const profiles = useProfiles();
  return (
    <Card title="App" icon={<SlidersHorizontal size={16} />}>
      <Row label="Start with Windows" icon={<Power size={16} />} htmlFor="sw-start">
        <Switch id="sw-start" label="Start with Windows" checked={settings.startWithOs} onChange={(v) => update((d) => void (d.startWithOs = v))} />
      </Row>
      <Row label="Theme" icon={<Palette size={16} />}>
        <Segmented
          label="Theme"
          value={settings.theme}
          options={[
            { value: "system", label: "System", icon: <Monitor size={13} /> },
            { value: "light", label: "Light", icon: <Sun size={13} /> },
            { value: "dark", label: "Dark", icon: <Moon size={13} /> },
          ]}
          onChange={(v) => update((d) => void (d.theme = v))}
        />
      </Row>
      <Row label="Color profile" icon={<Palette size={16} />} hint="The accent of the tray icon, the taskbar button and the app. Red for no microphone stays red.">
        <div className="swatches" role="radiogroup" aria-label="Color profile">
          {profiles.map((p) => (
            <button
              key={p.id}
              type="button"
              role="radio"
              className="swatch"
              aria-checked={settings.colorProfile === p.id}
              aria-label={p.label}
              title={p.label}
              style={{ background: p.accentDark }}
              onClick={() => update((d) => void (d.colorProfile = p.id))}
            >
              {settings.colorProfile === p.id && <Check size={13} color={p.inkDark} strokeWidth={3} aria-hidden />}
            </button>
          ))}
        </div>
      </Row>
      <Row label="Logs" icon={<FileText size={16} />} hint={info ? `Version ${info.version}. Logs never contain what you dictate.` : undefined} end>
        <button className="btn btn-sm" onClick={() => void ipc.openLogs()}>
          <FolderOpen size={13} /> Open logs
        </button>
      </Row>
    </Card>
  );
}
