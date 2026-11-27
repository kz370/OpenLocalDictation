import { useState } from "react";
import { CircleDot, Droplets, Lock, LockKeyhole, Maximize2, Move, MoveHorizontal, PanelBottom, PictureInPicture2, Pin, RotateCcw, Settings as SettingsIcon, X } from "lucide-react";
import { ipc } from "../../ipc";
import type { Settings, TaskbarStatus } from "../../types";
import { Dialog, Segmented, Switch } from "../../components/controls";
import { Card, Row } from "../../components/layout";
import type { Update } from "./shared";

export function OverlayPage({ settings, update, taskbar }: { settings: Settings; update: Update; taskbar: TaskbarStatus }) {
  const [confirmPosition, setConfirmPosition] = useState(false);
  return (
    <>
      <Card title="Appearance" icon={<PictureInPicture2 size={16} />}>
        <Row label="Overlay size" icon={<Maximize2 size={16} />} hint="Small is a compact pill; with both, a button in the overlay switches.">
          <Segmented
            label="Overlay size"
            value={settings.overlayLayouts}
            options={[
              { value: "small", label: "Small only" },
              { value: "large", label: "Large only" },
              { value: "both", label: "Both" },
            ]}
            onChange={(v) => update((d) => void (d.overlayLayouts = v))}
          />
        </Row>
        {settings.overlayLayouts !== "large" && (
          <Row
            label="Small overlay style"
            icon={<CircleDot size={16} />}
            hint="Pill: always shown. Bubble: only a mic button stays; the pill opens above it."
          >
            <Segmented
              label="Small overlay style"
              value={settings.overlayStyle}
              options={[
                { value: "pill", label: "Pill" },
                { value: "bubble", label: "Bubble" },
              ]}
              onChange={(v) => update((d) => void (d.overlayStyle = v))}
            />
          </Row>
        )}
        <Row label="Overlay opacity" icon={<Droplets size={16} />} hint="Makes the background see-through. Text and buttons stay solid.">
          <div style={{ display: "flex", alignItems: "center", gap: 10, width: 220 }}>
            <input type="range" min={30} max={100} step={5} aria-label="Overlay opacity" value={settings.overlayOpacity} onChange={(e) => update((d) => void (d.overlayOpacity = Number(e.target.value)))} />
            <span style={{ width: 40, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{settings.overlayOpacity}%</span>
          </div>
        </Row>
        <Row label="Settings button" icon={<SettingsIcon size={16} />} hint="Shows a gear in the overlay that opens these settings." htmlFor="sw-gear">
          <Switch id="sw-gear" label="Settings button" checked={settings.overlayShowSettings} onChange={(v) => update((d) => void (d.overlayShowSettings = v))} />
        </Row>
      </Card>

      <Card title="Always available" icon={<Pin size={16} />}>
        <Row
          label="Mic button in the taskbar"
          icon={<PanelBottom size={16} />}
          hint="Click to dictate; point at it to open the pill."
          info="Hold Ctrl and drag it to move it along the taskbar. Needs a taskbar at the bottom of the screen."
          htmlFor="sw-taskbar"
        >
          <Switch id="sw-taskbar" label="Mic button in the taskbar" checked={settings.taskbarWidget} onChange={(v) => update((d) => void (d.taskbarWidget = v))} />
        </Row>
        <Row
          label="Pin the overlay"
          icon={<Pin size={16} />}
          hint="Keeps the overlay on screen, above other apps."
          info="It listens to nothing until you press the shortcut or its mic button."
          htmlFor="sw-pin"
        >
          <Switch id="sw-pin" label="Pin the overlay" checked={settings.overlayPinned} onChange={(v) => void ipc.overlaySetPinned(v)} />
        </Row>
      </Card>

      <Card title="Position" icon={<Move size={16} />}>
        <Row label="Move sideways only" icon={<MoveHorizontal size={16} />} hint="Slides left and right and stays just above the taskbar." htmlFor="sw-side">
          <Switch id="sw-side" label="Move sideways only" checked={settings.overlayHorizontalOnly} onChange={(v) => update((d) => void (d.overlayHorizontalOnly = v))} />
        </Row>
        <Row label="Lock the overlay position" icon={<LockKeyhole size={16} />} hint="Stops the overlay from being dragged." htmlFor="sw-poslock">
          <Switch id="sw-poslock" label="Lock the overlay position" checked={settings.overlayPositionLocked} onChange={(v) => update((d) => void (d.overlayPositionLocked = v))} />
        </Row>
        <Row
          label="Lock the small overlay's size"
          icon={<Lock size={16} />}
          hint="Off: drag the small overlay's right edge to resize it."
          info="Double-click the edge to reset the width."
          htmlFor="sw-lock"
        >
          <Switch id="sw-lock" label="Lock the small overlay's size" checked={settings.overlaySizeLocked} onChange={(v) => update((d) => void (d.overlaySizeLocked = v))} />
        </Row>
        <Row
          label="Reset positions"
          icon={<RotateCcw size={16} />}
          hint={settings.taskbarWidget ? (taskbar.shown ? "Put the overlay and the taskbar button back at their default spots." : (taskbar.reason ?? "Placing the taskbar button…")) : "Drag the overlay by its header to move it."}
        >
          <button className="btn btn-sm" onClick={() => setConfirmPosition(true)}>
            <RotateCcw size={13} /> Reset position
          </button>
        </Row>
      </Card>

      {confirmPosition && (
        <Dialog
          title="Reset the overlay position?"
          onClose={() => setConfirmPosition(false)}
          actions={
            <>
              <button className="btn" onClick={() => setConfirmPosition(false)}>
                <X size={13} /> Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={() => {
                  setConfirmPosition(false);
                  void ipc.dictationResetOverlayPosition();
                  if (settings.taskbarWidget) void ipc.taskbarWidgetResetPosition();
                }}
              >
                <RotateCcw size={13} /> Reset position
              </button>
            </>
          }
        >
          <p>
            The overlay goes back to its default spot at the bottom centre of the screen
            {settings.taskbarWidget ? ", and the taskbar button goes back to the left of the notification area" : ""}.
          </p>
        </Dialog>
      )}
    </>
  );
}
