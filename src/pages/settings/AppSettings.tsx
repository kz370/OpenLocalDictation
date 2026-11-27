import { FileText, FolderOpen, Monitor, Moon, Palette, Power, SlidersHorizontal, Sun } from "lucide-react";
import { ipc } from "../../ipc";
import type { AppInfo, Settings } from "../../types";
import { Segmented, Switch } from "../../components/controls";
import { Card, Row } from "../../components/layout";
import type { Update } from "./shared";

export function AppPage({ settings, update, info }: { settings: Settings; update: Update; info: AppInfo | null }) {
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
      <Row label="Logs" icon={<FileText size={16} />} hint={info ? `Version ${info.version}. Logs never contain what you dictate.` : undefined} end>
        <button className="btn btn-sm" onClick={() => void ipc.openLogs()}>
          <FolderOpen size={13} /> Open logs
        </button>
      </Row>
    </Card>
  );
}
