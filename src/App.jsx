import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { motion, AnimatePresence } from "framer-motion";
import shiroLogo from "./assets/shiro.png";
import "./App.css";

const isPopup = new URLSearchParams(window.location.search).get("window") === "popup";

export default function App() {
  return isPopup ? <CapturePill /> : <MainApp />;
}

/* ── Icons (Lucide-style, stroke 1.75, currentColor) ───────────────────────── */
const S = (p) => ({ width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.75, strokeLinecap: "round", strokeLinejoin: "round", ...p });
const IconSearch = (p) => (<svg {...S(p)}><circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" /></svg>);
const IconSettings = (p) => (<svg {...S(p)}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" /></svg>);
const IconCamera = (p) => (<svg {...S(p)}><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3Z" /><circle cx="12" cy="13" r="3.5" /></svg>);
const IconAlarm = (p) => (<svg {...S(p)}><circle cx="12" cy="13" r="8" /><path d="M12 9v4l2 2" /><path d="M5 3 2 6" /><path d="m22 6-3-3" /><path d="M6.38 18.7 4 21" /><path d="M17.64 18.67 20 21" /></svg>);
const IconNote = (p) => (<svg {...S(p)}><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4Z" /></svg>);
const IconClose = (p) => (<svg {...S(p)}><path d="M18 6 6 18M6 6l12 12" /></svg>);
const IconBack = (p) => (<svg {...S(p)}><path d="m15 18-6-6 6-6" /></svg>);
const IconExternal = (p) => (<svg {...S(p)}><path d="M15 3h6v6" /><path d="M10 14 21 3" /><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /></svg>);
const IconTrash = (p) => (<svg {...S(p)}><path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" /></svg>);
const IconHighlight = (p) => (<svg {...S(p)}><path d="M12 20h9M2 20h2l10-10-2-2L2 18v2Z" /><path d="m12.5 6.5 3 3" /></svg>);
const IconLink = (p) => (<svg {...S(p)}><path d="M10 13a5 5 0 0 0 7 0l2-2a5 5 0 0 0-7-7l-1 1" /><path d="M14 11a5 5 0 0 0-7 0l-2 2a5 5 0 0 0 7 7l1-1" /></svg>);
const IconImage = (p) => (<svg {...S(p)}><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-5-5L5 21" /></svg>);
const IconFile = (p) => (<svg {...S(p)}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" /><path d="M14 2v6h6" /></svg>);

const KIND = {
  highlight: { label: "Highlight", Icon: IconHighlight, cls: "k-highlight" },
  link: { label: "Link", Icon: IconLink, cls: "k-link" },
  image: { label: "Image", Icon: IconImage, cls: "k-image" },
  file: { label: "File", Icon: IconFile, cls: "k-file" },
};
function kindOf(item) {
  if (item.type === "image" || (item.image_path && item.type !== "file")) {
    return item.type === "highlight" ? KIND.highlight : KIND.image;
  }
  return KIND[item.type] || KIND.file;
}

/* ── Main library ──────────────────────────────────────────────────────────── */
function MainApp() {
  const [items, setItems] = useState([]);
  const [search, setSearch] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [filter, setFilter] = useState("all");
  const [selectedId, setSelectedId] = useState(null);
  const [view, setView] = useState("library");
  const [dropping, setDropping] = useState(false);
  const searchRef = useRef(null);
  const searching = search.trim().length > 0;

  const load = useCallback(async () => {
    try {
      setItems(await invoke("cmd_get_items", { filter: null }));
    } catch (e) {
      console.error(e);
    }
  }, []);

  const runSearch = useCallback(async (q) => {
    try {
      setItems(await invoke("cmd_search", { query: q }));
    } catch (e) {
      console.error(e);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const refresh = useCallback(() => {
    if (search.trim()) runSearch(search);
    else load();
  }, [search, runSearch, load]);

  useEffect(() => {
    const uns = [];
    listen("item-saved", refresh).then((u) => uns.push(u));
    listen("storage-changed", () => { setSearch(""); setFilter("all"); load(); }).then((u) => uns.push(u));
    return () => uns.forEach((u) => u?.());
  }, [refresh, load]);

  // Drag & drop files onto the window (logo is the visual target).
  useEffect(() => {
    let un;
    getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") setDropping(true);
      else if (p.type === "leave") setDropping(false);
      else if (p.type === "drop") {
        setDropping(false);
        if (p.paths?.length) invoke("cmd_save_files", { paths: p.paths, notes: null }).catch(console.error);
      }
    }).then((u) => (un = u));
    return () => un?.();
  }, []);

  const onSearchChange = (q) => {
    setSearch(q);
    if (q.trim()) runSearch(q);
    else { setFilter("all"); load(); }
  };

  const toggleSearch = () => {
    const next = !searchOpen;
    setSearchOpen(next);
    if (next) setTimeout(() => searchRef.current?.focus(), 60);
    else onSearchChange("");
  };

  const visible = filter === "all"
    ? items
    : items.filter((it) => (filter === "image" ? !!it.image_path : it.type === filter));

  const selected = items.find((i) => i.id === selectedId) ?? null;

  const handleDelete = async (id) => {
    try { await invoke("cmd_delete_item", { id }); } catch (e) { console.error(e); }
    setSelectedId(null);
    refresh();
  };

  if (view === "settings") return <Settings onBack={() => setView("library")} />;

  return (
    <div className="app">
      <header className="topbar">
        <div className={`brand ${dropping ? "drop" : ""}`} title="Drop files here to save">
          <img src={shiroLogo} alt="Shiro" />
          {dropping && <span className="brand-hint">Drop to save</span>}
        </div>
        <div className="topbar-actions">
          <AnimatePresence initial={false} mode="wait">
            {searchOpen ? (
              <motion.div key="box" className="search-box"
                initial={{ width: 38, opacity: 0 }} animate={{ width: 280, opacity: 1 }} exit={{ width: 38, opacity: 0 }}
                transition={{ duration: 0.26, ease: [0.16, 1, 0.3, 1] }}>
                <IconSearch />
                <input ref={searchRef} placeholder="Search everything…" value={search}
                  onChange={(e) => onSearchChange(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Escape") toggleSearch(); }} />
                <button className="clear" onClick={toggleSearch} aria-label="Close search"><IconClose /></button>
              </motion.div>
            ) : (
              <motion.button key="btn" className="icon-btn" onClick={toggleSearch} title="Search" aria-label="Search"
                initial={{ opacity: 0, scale: 0.8 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.8 }}>
                <IconSearch />
              </motion.button>
            )}
          </AnimatePresence>
          <button className="icon-btn" onClick={() => setView("settings")} title="Settings" aria-label="Settings">
            <IconSettings />
          </button>
        </div>
      </header>

      {searching && (
        <div className="filters">
          {["all", "highlight", "link", "image", "file"].map((f) => (
            <button key={f} className={`chip ${filter === f ? "active" : ""}`} onClick={() => setFilter(f)}>
              {f === "all" ? "All" : KIND[f].label + "s"}
            </button>
          ))}
        </div>
      )}

      <main className="canvas">
        {visible.length === 0 ? (
          <div className="empty">
            <img src={shiroLogo} alt="" />
            <h3>{searching ? "Nothing matches" : "Nothing saved yet"}</h3>
            <p>{searching ? "Try a different search." : "Press your shortcut to capture text or a screenshot, or drop files onto the logo."}</p>
          </div>
        ) : (
          <div className="card-grid">
            {visible.map((it, i) => (
              <Card key={it.id} item={it} index={i} onClick={() => setSelectedId(it.id)} />
            ))}
          </div>
        )}
      </main>

      <AnimatePresence>
        {selected && (
          <DetailModal
            key={selected.id}
            item={selected}
            onClose={() => setSelectedId(null)}
            onDelete={handleDelete}
            onSaved={refresh}
          />
        )}
      </AnimatePresence>
    </div>
  );
}

function Card({ item, index, onClick }) {
  const { label, Icon, cls } = kindOf(item);
  const [thumb, setThumb] = useState(null);
  useEffect(() => {
    let alive = true;
    if (item.image_path) invoke("cmd_read_image", { path: item.image_path }).then((u) => alive && setThumb(u)).catch(() => {});
    return () => { alive = false; };
  }, [item.id]);

  const snippet = item.text || item.url || item.file_path || "";
  return (
    <motion.div
      className="card"
      onClick={onClick}
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.32, ease: [0.16, 1, 0.3, 1], delay: Math.min((index || 0) * 0.03, 0.25) }}
    >
      {thumb && <img className="card-thumb" src={thumb} alt="" />}
      <div className="card-body">
        <span className={`card-kind ${cls}`}><Icon /> {label}</span>
        <div className="card-title">{item.title || snippet || "Untitled"}</div>
        {item.title && snippet && <div className="card-snippet">{snippet}</div>}
        <div className="card-foot">
          {item.remind_at && <span className="bell"><IconAlarm /> {new Date(item.remind_at).toLocaleDateString()}</span>}
          <span style={{ marginLeft: "auto" }}>{relativeTime(item.created_at)}</span>
        </div>
      </div>
    </motion.div>
  );
}

function DetailModal({ item, onClose, onDelete, onSaved }) {
  const [notes, setNotes] = useState(item.notes ?? "");
  const [img, setImg] = useState(null);
  const { label, Icon, cls } = kindOf(item);

  useEffect(() => {
    setNotes(item.notes ?? "");
    setImg(null);
    if (item.image_path) invoke("cmd_read_image", { path: item.image_path }).then(setImg).catch(console.error);
    const onKey = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [item.id]);

  const saveNotes = () => { invoke("cmd_update_notes", { id: item.id, notes }).then(onSaved).catch(console.error); };

  return (
    <motion.div className="scrim" onClick={onClose}
      initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.16 }}>
      <motion.div className="modal" onClick={(e) => e.stopPropagation()}
        initial={{ opacity: 0, y: 14, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 10, scale: 0.98 }} transition={{ type: "spring", stiffness: 320, damping: 30 }}>
        <div className="modal-top">
          <div style={{ minWidth: 0 }}>
            <span className={`card-kind ${cls}`} style={{ marginBottom: 8 }}><Icon /> {label}</span>
            <div className="modal-title">{item.title || item.text?.slice(0, 80) || "Untitled"}</div>
            {item.url && item.type !== "file" && (
              <div className="modal-url" onClick={() => openUrl(item.url)}>{item.url}</div>
            )}
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="Close"><IconClose /></button>
        </div>

        {img && <img className="modal-img" src={img} alt="screenshot" onClick={() => invoke("cmd_open_path", { path: item.image_path })} title="Open full image" />}
        {item.text && <div className="modal-text">{item.text}</div>}

        <div className="label">Notes</div>
        <textarea className="notes-area" value={notes} onChange={(e) => setNotes(e.target.value)} onBlur={saveNotes} placeholder="Add a note…" rows={4} />

        <div className="modal-meta">
          Saved {new Date(item.created_at).toLocaleString()}
          {item.remind_at && ` · Reminder ${new Date(item.remind_at).toLocaleString()}`}
        </div>

        <div className="modal-actions">
          {item.file_path && (
            <button className="btn btn-ghost" onClick={() => invoke("cmd_open_path", { path: item.file_path })}><IconExternal /> Open</button>
          )}
          {item.url && item.type !== "file" && (
            <button className="btn btn-ghost" onClick={() => openUrl(item.url)}><IconExternal /> Open original</button>
          )}
          <button className="btn btn-danger" style={{ marginLeft: "auto" }} onClick={() => onDelete(item.id)}><IconTrash /> Delete</button>
        </div>
      </motion.div>
    </motion.div>
  );
}

/* ── Settings ──────────────────────────────────────────────────────────────── */
function Settings({ onBack }) {
  const [hotkey, setHotkey] = useState("Meta+KeyE");
  const [listening, setListening] = useState(false);
  const [saved, setSaved] = useState(false);
  const [storageDir, setStorageDir] = useState("");
  const [moving, setMoving] = useState(false);
  const [axTrusted, setAxTrusted] = useState(true);

  useEffect(() => {
    invoke("cmd_get_hotkey").then(setHotkey).catch(console.error);
    invoke("cmd_get_storage_dir").then(setStorageDir).catch(console.error);
  }, []);

  const checkAx = useCallback(() => { invoke("cmd_accessibility_status").then(setAxTrusted).catch(console.error); }, []);
  useEffect(() => { checkAx(); const id = setInterval(checkAx, 2000); return () => clearInterval(id); }, [checkAx]);

  const handleKeyDown = (e) => {
    if (!listening) return;
    e.preventDefault(); e.stopPropagation();
    if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return;
    const parts = [];
    if (e.metaKey) parts.push("Meta");
    if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    parts.push(e.code);
    setHotkey(parts.join("+"));
    setListening(false);
  };
  const save = async () => {
    try { await invoke("cmd_set_hotkey", { hotkey }); setSaved(true); setTimeout(() => setSaved(false), 2000); }
    catch (e) { console.error(e); }
  };
  const changeLocation = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose where Shiro stores everything" });
    if (!picked || picked === storageDir) return;
    setMoving(true);
    try { await invoke("cmd_set_storage_dir", { newDir: picked }); setStorageDir(picked); }
    catch (e) { console.error(e); }
    setMoving(false);
  };
  const enableAx = async () => { await invoke("cmd_request_accessibility"); setTimeout(checkAx, 500); };

  return (
    <div className="settings" onKeyDown={handleKeyDown} tabIndex={-1} style={{ outline: "none" }}>
      <div className="settings-inner">
        <div className="settings-h">
          <button className="icon-btn" onClick={onBack} aria-label="Back"><IconBack /></button>
          <div className="settings-title">Settings</div>
        </div>

        <div className="sec-label">Capture</div>
        <div className="row">
          <div>
            <div className="row-label">Global shortcut</div>
            <div className="row-sub">Press this anywhere to open the capture pill</div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div className={`hotkey-badge ${listening ? "listening" : ""}`} tabIndex={0}
              onClick={() => { setListening(true); setSaved(false); }}
              onFocus={() => { setListening(true); setSaved(false); }}
              onBlur={() => setListening(false)}>
              {listening ? "Press keys…" : shortcutDisplay(hotkey)}
            </div>
            <button className="btn btn-primary" onClick={save}>Save</button>
            {saved && <span className="saved">✓ Saved</span>}
          </div>
        </div>
        <div className="hint">Click the badge, then press your combination (e.g. ⌘E). Needs one modifier (⌘ ⌃ ⌥ ⇧).</div>

        <div className="sec-label">Storage</div>
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="row-label">Save location</div>
            <div className="row-sub" style={{ wordBreak: "break-all" }} title={storageDir}>{storageDir || "…"}</div>
          </div>
          <button className="btn btn-ghost" onClick={changeLocation} disabled={moving} style={{ whiteSpace: "nowrap" }}>
            {moving ? "Moving…" : "Change…"}
          </button>
        </div>
        <div className="hint">Everything — highlights, links, images, files — lives here as readable files. Changing this moves your data.</div>

        <div className="sec-label">Permissions</div>
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="row-label">Accessibility &amp; Screen Recording</div>
            <div className="row-sub">Needed to read selected text and capture screenshots.</div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span className={axTrusted ? "status-ok" : "status-no"}>{axTrusted ? "✓ Granted" : "Not granted"}</span>
            {!axTrusted && <button className="btn btn-primary" onClick={enableAx}>Enable…</button>}
          </div>
        </div>
        <div className="hint">
          Turn on <strong>Shiro</strong> under both lists, or <span className="linklike" onClick={() => invoke("cmd_open_accessibility_settings")}>open settings</span>.
          In <code>tauri dev</code> the grant resets on rebuild — use a real build to make it stick.
        </div>
      </div>
    </div>
  );
}

/* ── Capture pill (popup window) ───────────────────────────────────────────── */
function CapturePill() {
  const [data, setData] = useState({ url: null, title: null, text: null });
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [shown, setShown] = useState(false);
  const [shot, setShot] = useState(null);
  const [freeMode, setFreeMode] = useState(false);
  const [remindOpen, setRemindOpen] = useState(false);
  const [remindAt, setRemindAt] = useState("");
  const [noteOpen, setNoteOpen] = useState(false);
  const noteRef = useRef(null);
  const textRef = useRef(null);

  const reset = () => {
    setShown(false); setData({ url: null, title: null, text: null, files: [] });
    setNote(""); setShot(null); setFreeMode(false); setRemindOpen(false); setRemindAt(""); setNoteOpen(false); setSaving(false);
  };

  const requestClose = useCallback((restoreFocus = true) => {
    setShown(false);
    setTimeout(() => invoke("cmd_close_popup", { restoreFocus }), 150);
  }, []);

  useEffect(() => {
    const uns = [];
    listen("popup-data", (e) => {
      setData({ files: [], ...e.payload });
      setNote(""); setSaving(false); setRemindOpen(false); setRemindAt(""); setNoteOpen(false);
      setShot(e.payload.screenshot ? { path: e.payload.screenshot_path, dataUrl: e.payload.screenshot } : null);
      const free = !(e.payload.url || e.payload.text || e.payload.files?.length);
      setFreeMode(free);
      setShown(true);
      setTimeout(() => (free ? textRef : noteRef).current?.focus(), 0);
    }).then((u) => uns.push(u));
    listen("popup-reset", reset).then((u) => uns.push(u));
    listen("popup-dismiss", () => requestClose(false)).then((u) => uns.push(u));
    return () => uns.forEach((u) => u?.());
  }, [requestClose]);

  const attachScreenshot = async () => {
    try { const s = await invoke("cmd_take_screenshot"); if (s) setShot({ path: s.path, dataUrl: s.dataUrl }); }
    catch (e) { console.error(e); }
  };

  const handleSave = async () => {
    if (saving) return;
    const hasFiles = data.files?.length > 0;
    if (!hasFiles && !data.url && !data.text?.trim() && !shot) return;
    setSaving(true);
    const remind = remindAt ? new Date(remindAt).toISOString() : null;
    try {
      if (hasFiles) {
        await invoke("cmd_save_files", { paths: data.files, notes: note || null });
      } else {
        const type = data.text?.trim() ? "highlight" : data.url ? "link" : "highlight";
        await invoke("cmd_save_item", {
          req: { type, url: data.url ?? null, title: data.title ?? null, text: data.text ?? null, html: null, file_path: null, notes: note || null, remind_at: remind },
          screenshotPath: shot?.path ?? null,
        });
      }
      requestClose(true);
    } catch (e) { console.error("save failed", e); setSaving(false); }
  };

  const handleKeyDown = (e) => {
    if (e.key === "Escape") requestClose(true);
    if (e.key === "Enter" && (e.metaKey || e.target.tagName === "INPUT")) { e.preventDefault(); handleSave(); }
  };

  return (
    <div className="pill" onKeyDown={handleKeyDown}
      style={{ opacity: shown ? 1 : 0, transform: shown ? "scale(1)" : "scale(.97)", transformOrigin: "center", transition: "opacity .15s var(--ease), transform .15s var(--ease)" }}>
      <div className="pill-head">
        <img src={shiroLogo} alt="Shiro" />
        <span className="spacer" />
      </div>

      <div className="pill-content">
        {data.files?.length > 0 && (
          <div className="pill-files">📎 {data.files.length === 1 ? baseName(data.files[0]) : `${data.files.length} files`}</div>
        )}
        {data.title && <div className="pill-title">{data.title}</div>}
        {data.url && <div className="pill-url">{data.url}</div>}
        {!freeMode && data.text && <div className="pill-quote">{data.text}</div>}

        {freeMode && (
          <textarea ref={textRef} className="pinput pill-capture" rows={2} placeholder="Type or paste anything…"
            value={data.text || ""} onChange={(e) => setData((d) => ({ ...d, text: e.target.value }))} />
        )}

        {shot && (
          <div className="pill-shot">
            <img src={shot.dataUrl} alt="screenshot" />
            <button className="shot-x" onClick={() => setShot(null)} aria-label="Remove screenshot"><IconClose /></button>
          </div>
        )}

        <AnimatePresence initial={false}>
          {(noteOpen || note) && (
            <motion.input key="note" ref={noteRef} className="pinput pill-note" placeholder="Add a note…"
              value={note} onChange={(e) => setNote(e.target.value)}
              initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }} />
          )}
          {remindOpen && (
            <motion.input key="dt" type="datetime-local" className="dt-input" value={remindAt}
              onChange={(e) => setRemindAt(e.target.value)}
              initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }} />
          )}
        </AnimatePresence>
      </div>

      <div className="pill-foot">
        {!data.files?.length && (
          <button className={`icon-btn ${shot ? "active" : ""}`} onClick={attachScreenshot} title="Screenshot"><IconCamera /></button>
        )}
        <button className={`icon-btn ${noteOpen || note ? "active" : ""}`}
          onClick={() => { setNoteOpen(true); setTimeout(() => noteRef.current?.focus(), 0); }} title="Note"><IconNote /></button>
        {!data.files?.length && (
          <button className={`icon-btn ${remindOpen || remindAt ? "active" : ""}`} onClick={() => setRemindOpen((v) => !v)} title="Reminder"><IconAlarm /></button>
        )}
        <span className="spacer" />
        <button className="btn btn-primary" onClick={handleSave} disabled={saving}>{saving ? "Saving…" : "Save ↵"}</button>
      </div>
    </div>
  );
}

/* ── Utils ─────────────────────────────────────────────────────────────────── */
function shortcutDisplay(str) {
  if (!str) return "";
  const m = { Meta: "⌘", Shift: "⇧", Alt: "⌥", Control: "⌃", Ctrl: "⌃" };
  return str.split("+").map((p) => (m[p] || (p.startsWith("Key") ? p.slice(3) : p.startsWith("Digit") ? p.slice(5) : p))).join("");
}
function baseName(path) { return path?.split("/").pop() || path || ""; }
function relativeTime(iso) {
  const diff = Date.now() - new Date(iso).getTime();
  const min = Math.floor(diff / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min}m ago`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d ago`;
  return new Date(iso).toLocaleDateString();
}
