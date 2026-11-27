import { Fragment, useEffect, useMemo, useState } from "react";
import { Check, ListFilter, Languages, Pencil, RotateCcw, X } from "lucide-react";
import { ipc, on } from "../../ipc";
import { KNOWN_LANGUAGES } from "../../knownLanguages";
import type { ModelEntry, Settings } from "../../types";
import { Dialog, Segmented } from "../../components/controls";
import { Card, Row } from "../../components/layout";
import { SearchSelect, type SearchOption } from "../../components/SearchSelect";
import { langLabel, languageOptions, resolveActiveModel, shortModel, type Update } from "./shared";

/** All languages, or a chosen few, in the overlay's language menu. */
function LanguageFilter({ shown, onChange }: { shown: string[]; onChange: (v: string[]) => void }) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const all = shown.length === 0;
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return KNOWN_LANGUAGES.filter((l) => !q || `${l.name} ${l.native} ${l.code}`.toLowerCase().includes(q));
  }, [query]);
  const edit = () => {
    setDraft(shown);
    setQuery("");
    setOpen(true);
  };
  const toggle = (code: string) => setDraft((d) => (d.includes(code) ? d.filter((c) => c !== code) : [...d, code]));
  return (
    <>
      <Segmented
        label="Languages in the overlay menu"
        value={all ? "all" : "some"}
        options={[
          { value: "all", label: "All" },
          { value: "some", label: shown.length ? `Chosen (${shown.length})` : "Chosen…" },
        ]}
        onChange={(v) => (v === "all" ? onChange([]) : edit())}
      />
      {!all && (
        <button className="btn btn-sm" onClick={edit}>
          <Pencil size={13} /> Edit…
        </button>
      )}
      {open && (
        <Dialog
          title="Languages in the overlay menu"
          onClose={() => setOpen(false)}
          actions={
            <>
              <button className="btn" onClick={() => setOpen(false)}>
                <X size={13} /> Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={draft.length === 0}
                onClick={() => {
                  onChange(draft);
                  setOpen(false);
                }}
              >
                <Check size={13} /> Save
              </button>
            </>
          }
        >
          <input className="input" placeholder="Search languages…" aria-label="Search languages" value={query} onChange={(e) => setQuery(e.target.value)} />
          <div className="lang-pick">
            {[...rows.filter((l) => draft.includes(l.code)), ...rows.filter((l) => !draft.includes(l.code))].map((l, i, arr) => (
              <Fragment key={l.code}>
                {i > 0 && draft.includes(arr[i - 1].code) && !draft.includes(l.code) && <div className="lang-pick-divider" role="separator" />}
                <label className="lang-pick-item">
                  <input type="checkbox" checked={draft.includes(l.code)} onChange={() => toggle(l.code)} />
                  {l.name === l.native ? l.name : `${l.name} · ${l.native}`}
                </label>
              </Fragment>
            ))}
          </div>
          <p className="row-hint">{draft.length === 0 ? "Pick at least one language, or choose All." : `${draft.length} selected. Automatic is always shown.`}</p>
        </Dialog>
      )}
    </>
  );
}

/**
 * A model per language. Lists the languages chosen for the overlay
 * menu (chosen above), or every language when that menu shows all of them.
 * Several rows can be ticked and given the same model at once. Changes save
 * like every other setting.
 */
function LanguageModelsCard({ settings, update }: { settings: Settings; update: (m: (d: Settings) => void) => void }) {
  const [models, setModels] = useState<ModelEntry[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  const [query, setQuery] = useState("");

  useEffect(() => {
    const refresh = () => void ipc.modelsList().then(setModels).catch(() => undefined);
    refresh();
    const sub = on("models://changed", refresh);
    return () => void sub.then((u) => u());
  }, []);

  const allInstalled = models.filter((m) => m.installed);
  // Same rule as the backend (and the Speech model tab) for the model used for all languages.
  const fallbackId = resolveActiveModel(models, settings);
  // Hidden models are not offered here, unless one is already in use (the default, or chosen for a language).
  const inUse = new Set([fallbackId, ...Object.values(settings.languageModels ?? {})]);
  const installed = allInstalled.filter((m) => !(settings.hiddenModels ?? []).includes(m.id) || inUse.has(m.id));
  const defaultName = shortModel(installed.find((m) => m.id === fallbackId)?.name ?? "none installed");
  const modelName = (id: string) => shortModel(installed.find((m) => m.id === id)?.name ?? id);
  const modelOptions: SearchOption[] = installed.map((m) => ({ value: m.id, label: shortModel(m.name) }));
  const withDefault: SearchOption[] = [{ value: "", label: `Default (${defaultName})` }, ...modelOptions];

  const map = settings.languageModels ?? {};
  const chosen = settings.shownLanguages.length > 0;
  const total = chosen ? new Set([...settings.shownLanguages, ...Object.keys(map)]).size : KNOWN_LANGUAGES.length;
  // Chosen overlay languages (plus any that already have a model), or all languages.
  const listed = (chosen ? [...settings.shownLanguages, ...Object.keys(map).filter((c) => !settings.shownLanguages.includes(c))] : KNOWN_LANGUAGES.map((l) => l.code)).filter((c) => {
    const q = total > 5 ? query.trim().toLowerCase() : "";
    return !q || `${langLabel(c)} ${c}`.toLowerCase().includes(q);
  });
  const searchable = total > 5;
  // One compact drop-down per language (searchable above five models): it never wraps, however many models there are.
  const allPicked = listed.length > 0 && listed.every((c) => picked.includes(c));
  const toggle = (code: string) => setPicked((p) => (p.includes(code) ? p.filter((c) => c !== code) : [...p, code]));
  const setMany = (list: string[], id: string) =>
    update((d) => {
      const next = { ...(d.languageModels ?? {}) };
      for (const c of list) {
        if (id) next[c] = id;
        else delete next[c];
      }
      d.languageModels = next;
    });
  const overridden = Object.keys(map).length;

  return (
    <Card
      title="Model per language"
      icon={<Languages size={16} />}
      actions={
        <button className="btn btn-sm" disabled={overridden === 0} onClick={() => update((d) => void (d.languageModels = {}))}>
          <RotateCcw size={13} /> Same model for all
        </button>
      }
    >
      <p className="row-hint" style={{ margin: "10px 0 2px" }}>
        A bigger model transcribes hard languages (Arabic, Hindi, Thai…) far better; a small, fast one is fine for easy languages.
      </p>
      <p className="row-hint" style={{ margin: "0 0 10px" }}>
        {chosen ? "Listed: the languages chosen for the overlay menu above." : "Listed: all languages. Choose a few for the overlay menu above to shorten the list."} Automatic detection uses the default model.
      </p>
      {installed.length === 0 && <div className="notice warn" style={{ margin: "8px 0" }}>No model is installed yet. Download one on the Models page first.</div>}
      <div className="lang-model-bar">
        {searchable && <input className="input lang-model-search" placeholder={`Search ${total} languages…`} aria-label="Search languages" value={query} onChange={(e) => setQuery(e.target.value)} />}
        <label className="lang-pick-item">
          <input type="checkbox" checked={allPicked} disabled={listed.length === 0} onChange={() => setPicked(allPicked ? picked.filter((c) => !listed.includes(c)) : [...new Set([...picked, ...listed])])} />
          {picked.length > 0 ? `${picked.length} selected` : "Select all"}
        </label>
        <div className="lang-model-bulk-select">
          <SearchSelect label="Model for the selected languages" searchable={installed.length > 5} value="__pick" options={[{ value: "__pick", label: picked.length > 0 ? `Set model for ${picked.length} selected…` : "Tick languages to set them together" }, ...withDefault.map((o) => (o.value === "" ? { ...o, value: "__default" } : o))]} onChange={(v) => picked.length > 0 && v !== "__pick" && setMany(picked, v === "__default" ? "" : v)} />
        </div>
      </div>
      <div className="lang-model-list">
        {listed.map((code) => (
          <div key={code} className={`lang-model-row${picked.includes(code) ? " picked" : ""}`}>
            <label className="lang-pick-item">
              <input type="checkbox" checked={picked.includes(code)} onChange={() => toggle(code)} />
              <span className="lang-model-name">{langLabel(code)}</span>
            </label>
            <SearchSelect label={`Model for ${langLabel(code)}`} searchable={installed.length > 5} value={map[code] ?? ""} options={map[code] && !modelOptions.some((o) => o.value === map[code]) ? [...withDefault, { value: map[code], label: `${modelName(map[code])} (not installed)` }] : withDefault} onChange={(v) => setMany([code], v)} />
          </div>
        ))}
        {listed.length === 0 && <p className="row-hint" style={{ padding: 12, margin: 0 }}>No language matches.</p>}
      </div>
    </Card>
  );
}

export function LanguagesPage({ settings, update }: { settings: Settings; update: Update }) {
  return (
    <>
      <Card title="Dictation language" icon={<Languages size={16} />}>
        <Row label="Language" icon={<Languages size={16} />} hint={settings.shownLanguages?.length ? "Only your chosen languages are listed." : "Also changeable in the overlay while dictating."}>
          <SearchSelect
            label="Language"
            placeholder="Search languages…"
            value={settings.language}
            options={settings.shownLanguages?.length ? languageOptions.filter((o) => o.value === "auto" || o.value === settings.language || settings.shownLanguages.includes(o.value)) : languageOptions}
            onChange={(v) => update((d) => void (d.language = v))}
          />
        </Row>
        <Row label="Languages in the overlay menu" icon={<ListFilter size={16} />} hint="Shorten the overlay's language list to the ones you use.">
          <LanguageFilter shown={settings.shownLanguages ?? []} onChange={(v) => update((d) => void (d.shownLanguages = v))} />
        </Row>
      </Card>
      <LanguageModelsCard settings={settings} update={update} />
    </>
  );
}
