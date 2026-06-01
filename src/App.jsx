import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { motion, AnimatePresence, LayoutGroup, useMotionValue, animate as fmAnimate } from "framer-motion";
import shiroLogo from "./assets/shiro.png";
import "./App.css";

const isPopup = new URLSearchParams(window.location.search).get("window") === "popup";
// Only the popup window is transparent (for its rounded card). The main window
// stays opaque so maximize/resize doesn't flash a black backing.
if (isPopup) document.documentElement.classList.add("popup");

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
const IconChevRR  = (p) => (<svg {...S(p)}><path d="m6 17 5-5-5-5"/><path d="m13 17 5-5-5-5"/></svg>);
const IconChevLL  = (p) => (<svg {...S(p)}><path d="m18 17-5-5 5-5"/><path d="m11 17-5-5 5-5"/></svg>);
const IconBack = (p) => (<svg {...S(p)}><path d="m15 18-6-6 6-6" /></svg>);
const IconExternal = (p) => (<svg {...S(p)}><path d="M15 3h6v6" /><path d="M10 14 21 3" /><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /></svg>);
const IconTrash = (p) => (<svg {...S(p)}><path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" /></svg>);
const IconHighlight = (p) => (<svg {...S(p)}><path d="M12 20h9M2 20h2l10-10-2-2L2 18v2Z" /><path d="m12.5 6.5 3 3" /></svg>);
const IconLink = (p) => (<svg {...S(p)}><path d="M10 13a5 5 0 0 0 7 0l2-2a5 5 0 0 0-7-7l-1 1" /><path d="M14 11a5 5 0 0 0-7 0l-2 2a5 5 0 0 0 7 7l1-1" /></svg>);
const IconImage = (p) => (<svg {...S(p)}><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-5-5L5 21" /></svg>);
const IconFile = (p) => (<svg {...S(p)}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" /><path d="M14 2v6h6" /></svg>);
const IconMusic = (p) => (<svg {...S(p)}><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></svg>);
const IconVideo = (p) => (<svg {...S(p)}><path d="m22 8-6 4 6 4V8Z" /><rect x="2" y="6" width="14" height="12" rx="2" /></svg>);
const IconDoc = (p) => (<svg {...S(p)}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" /><path d="M14 2v6h6" /><path d="M16 13H8" /><path d="M16 17H8" /><path d="M10 9H8" /></svg>);
const IconSheet = (p) => (<svg {...S(p)}><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M3 9h18M3 15h18M9 3v18M15 3v18" /></svg>);
const IconSlides = (p) => (<svg {...S(p)}><rect x="2" y="3" width="20" height="14" rx="2" /><path d="M8 21h8M12 17v4" /></svg>);
const IconArchive = (p) => (<svg {...S(p)}><rect x="2" y="4" width="20" height="5" rx="1" /><path d="M4 9v9a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9" /><path d="M10 13h4" /></svg>);
const IconCode = (p) => (<svg {...S(p)}><path d="m16 18 6-6-6-6M8 6l-6 6 6 6" /></svg>);

const FILE_KINDS = {
  audio:   { label: "Audio",        Icon: IconMusic,  exts: ["mp3","wav","m4a","aac","flac","ogg","oga","opus","aiff","aif","alac","wma"] },
  video:   { label: "Video",        Icon: IconVideo,  exts: ["mp4","mov","m4v","mkv","webm","avi","wmv","flv","mpeg","mpg","3gp","ogv"] },
  pdf:     { label: "PDF",          Icon: IconDoc,    exts: ["pdf"] },
  doc:     { label: "Document",     Icon: IconDoc,    exts: ["doc","docx","txt","rtf","odt","pages","md","tex"] },
  sheet:   { label: "Spreadsheet",  Icon: IconSheet,  exts: ["xls","xlsx","csv","tsv","ods","numbers"] },
  slides:  { label: "Presentation", Icon: IconSlides, exts: ["ppt","pptx","odp","key"] },
  archive: { label: "Archive",      Icon: IconArchive,exts: ["zip","rar","7z","tar","gz","bz2","xz","dmg","iso"] },
  code:    { label: "Code",         Icon: IconCode,   exts: ["js","jsx","ts","tsx","py","rs","go","java","c","cpp","h","css","html","json","xml","yml","yaml","sh","rb","php","swift","kt"] },
};
function fileKind(path) {
  const ext = (path?.split(".").pop() || "").toLowerCase();
  for (const k in FILE_KINDS) if (FILE_KINDS[k].exts.includes(ext)) return { cat: k, ext, ...FILE_KINDS[k] };
  return { cat: "generic", ext, label: ext ? ext.toUpperCase() + " file" : "File", Icon: IconFile };
}

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

  // Drag & drop files anywhere on the window.
  useEffect(() => {
    let un;
    let disposed = false;
    getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") setDropping(true);
      else if (p.type === "leave") setDropping(false);
      else if (p.type === "drop") {
        setDropping(false);
        if (p.paths?.length) invoke("cmd_save_files", { paths: p.paths, notes: null }).catch(console.error);
      }
    }).then((u) => { if (disposed) u(); else un = u; });
    // Guard the async-cleanup race so we never end up with two listeners
    // (which double-saved every dropped file).
    return () => { disposed = true; un?.(); };
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

  return (
    <>
    <div className="app" aria-hidden={view === "settings"}
      style={view === "settings" ? { visibility: "hidden", pointerEvents: "none" } : undefined}>
      <header className="topbar" data-tauri-drag-region>
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

      <img className="brand-float" src={shiroLogo} alt="Shiro" />

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
            <p>{searching ? "Try a different search." : "Press your shortcut to capture text or a screenshot, or drop files anywhere."}</p>
          </div>
        ) : (
          <MasonryGrid
            items={visible}
            renderCard={(it, i) => (
              <Card key={it.id} item={it} index={i} onClick={() => setSelectedId(it.id)} />
            )}
          />
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

      <AnimatePresence>
        {dropping && (
          <motion.div className="drop-overlay"
            initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
            transition={{ duration: 0.14 }}>
            <div className="drop-card">Drop files to save</div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>

    <AnimatePresence>
      {view === "settings" && (
        <motion.div className="settings-overlay" key="settings"
          initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
          transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}>
          <Settings onBack={() => setView("library")} />
        </motion.div>
      )}
    </AnimatePresence>
    </>
  );
}

/* True masonry: responsive column count from the actual width, cards flow
   left-to-right (newest first) so reading order is preserved, each column
   stacks naturally so heights vary like a mood board. */
const MASONRY_EASE = [0.16, 1, 0.3, 1];
const MASONRY_DUR  = 0.42;

function MasonryGrid({ items, renderCard }) {
  const ref = useRef(null);
  const [cols, setCols] = useState(4);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const GAP = 14, MIN_COL = 228;
    let tid;
    const recompute = () => {
      clearTimeout(tid);
      // 120 ms debounce — fires once the user stops dragging the resize handle
      tid = setTimeout(() => {
        const w = el.clientWidth;
        if (w === 0) return; // hidden (e.g. settings open) — keep current column count
        setCols(Math.max(1, Math.min(6, Math.floor((w + GAP) / (MIN_COL + GAP)))));
      }, 120);
    };
    recompute();
    const ro = new ResizeObserver(recompute);
    ro.observe(el);
    return () => { ro.disconnect(); clearTimeout(tid); };
  }, []);

  const columns = Array.from({ length: cols }, () => []);
  items.forEach((it, i) => columns[i % cols].push([it, i]));

  return (
    <LayoutGroup id="masonry">
      <motion.div className="masonry" ref={ref} layout transition={{ duration: MASONRY_DUR, ease: MASONRY_EASE }}>
        {columns.map((col, ci) => (
          <motion.div className="masonry-col" key={ci} layout
            transition={{ duration: MASONRY_DUR, ease: MASONRY_EASE }}>
            {col.map(([it, i]) => renderCard(it, i))}
          </motion.div>
        ))}
      </motion.div>
    </LayoutGroup>
  );
}

function Card({ item, index, onClick }) {
  const { label, Icon, cls } = kindOf(item);
  const [thumb, setThumb] = useState(null);
  useEffect(() => {
    let alive = true;
    // Load thumbnail for screenshots, and also for image-extension files saved via drag-drop / Finder.
    const imgPath = item.image_path
      || (item.type === "file" && isImagePath(item.file_path) ? item.file_path : null);
    if (imgPath) invoke("cmd_read_image", { path: imgPath }).then((u) => alive && setThumb(u)).catch(() => {});
    return () => { alive = false; };
  }, [item.id]);

  // A URL copied as plain text (highlight type) should display like a link.
  const textIsUrl = /^https?:\/\//i.test((item.text || "").trim());
  const urlForLink = item.url || (textIsUrl ? item.text : null);
  const isLink = !!(urlForLink && (item.type === "link" || textIsUrl));
  const isFile = item.type === "file" && !isImagePath(item.file_path) && !thumb;

  // Never show raw URLs as text — always prettify.
  const displaySnippet = textIsUrl
    ? prettyUrl(item.text)
    : item.text || (item.type !== "file" ? prettyUrl(item.url) : null) || "";

  const foot = (
    <div className="card-foot">
      {item.remind_at && <span className="bell"><IconAlarm /> {new Date(item.remind_at).toLocaleDateString()}</span>}
      <span style={{ marginLeft: "auto" }}>{relativeTime(item.created_at)}</span>
    </div>
  );

  let body;
  if (isLink) {
    const li = linkInfo(urlForLink);
    body = (
      <div className="card-body">
        <div className="card-title">{item.title || li.domain}</div>
        <div className="card-snippet">{li.domain}{li.path ? li.path.slice(0, 48) + (li.path.length > 48 ? "…" : "") : ""}</div>
        {foot}
      </div>
    );
  } else if (isFile) {
    const fk = fileKind(item.file_path || item.title);
    body = (
      <>
        <div className="filecard-banner">
          {fk.cat === "audio" && <span className="filecard-wave" aria-hidden />}
          <span className="filecard-glyph"><fk.Icon /></span>
        </div>
        <div className="card-body">
          <div className="card-title">{item.title || baseName(item.file_path) || "File"}</div>
          <div className="card-foot">
            <span style={{ color: "var(--text3)", fontSize: 12 }}>{fk.label}</span>
            {item.remind_at && <span className="bell"><IconAlarm /> {new Date(item.remind_at).toLocaleDateString()}</span>}
            <span style={{ marginLeft: "auto" }}>{relativeTime(item.created_at)}</span>
          </div>
        </div>
      </>
    );
  } else if (thumb) {
    body = <img className="card-thumb" src={thumb} alt="" />;
  } else {
    body = (
      <div className="card-body">
        <div className="card-title">{item.title || displaySnippet || "Untitled"}</div>
        {item.title && displaySnippet && <div className="card-snippet">{displaySnippet}</div>}
        <div className="card-foot">
          {item.remind_at && <span className="bell"><IconAlarm /> {new Date(item.remind_at).toLocaleDateString()}</span>}
          <span style={{ marginLeft: "auto" }}>{relativeTime(item.created_at)}</span>
        </div>
      </div>
    );
  }

  const flat = isLink || isFile;
  return (
    <motion.div
      className={`card${!thumb && !flat ? " card-text" : ""}`}
      onClick={onClick}
      layout
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{
        duration: 0.28, ease: [0.16, 1, 0.3, 1], delay: Math.min((index || 0) * 0.025, 0.18),
        layout: { duration: MASONRY_DUR, ease: MASONRY_EASE },
      }}
    >
      {body}
    </motion.div>
  );
}

/* ── Helpers for floating notes ────────────────────────────────────────────── */
function parseNoteCards(raw) {
  if (!raw) return [];
  try {
    const arr = JSON.parse(raw);
    if (Array.isArray(arr)) return arr;
  } catch {}
  return raw.trim() ? [{ id: "legacy-0", text: raw }] : [];
}
function serializeNoteCards(cards) {
  return cards.length ? JSON.stringify(cards.map(({ id, text }) => ({ id, text }))) : "";
}

const STICKY_COLORS = ["#FEFCE8", "#FFF0F5", "#F0FDF4", "#EFF6FF"];

// Fixed anchor list — all positions kept to the sides of the image,
// within reasonable viewport bounds (no off-screen spawning).
function getAnchor(index) {
  const positions = [
    { x: -430, y: -170, rot: -5 },
    { x:  430, y: -160, rot:  4 },
    { x: -430, y:  180, rot:  6 },
    { x:  430, y:  170, rot: -3 },
    { x: -430, y:    0, rot: -2 },
    { x:  430, y:   10, rot:  3 },
    { x: -530, y: -100, rot: -4 },
    { x:  530, y:  -90, rot:  5 },
    { x: -530, y:  110, rot:  5 },
    { x:  530, y:  100, rot: -4 },
    { x: -430, y: -280, rot: -3 },
    { x:  430, y:  270, rot:  3 },
  ];
  const a   = positions[index % positions.length];
  const lap = Math.floor(index / positions.length);
  // Each full cycle just nudges y slightly so notes don't land exactly on top of each other.
  return { x: a.x, y: a.y + lap * 44, rot: a.rot };
}

function StickyNote({ card, index, onTextChange, onDelete, peekMode, href }) {
  const anchor  = getAnchor(index);
  const color   = href ? "#E8F0FE" : STICKY_COLORS[index % STICKY_COLORS.length];
  let hrefDomain = href || "";
  try { if (href) hrefDomain = new URL(href).hostname.replace(/^www\./, ""); } catch {}
  const phase   = index * 1.05;
  const peekDir = anchor.x < 0 ? -220 : 220;

  const xMv   = useMotionValue(anchor.x);
  const yMv   = useMotionValue(anchor.y);
  const rotMv = useMotionValue(anchor.rot);

  const [dragging, setDragging] = useState(false);
  const baseX = useRef(anchor.x);
  const baseY = useRef(anchor.y);

  // Wave animation — pauses while dragging.
  useEffect(() => {
    if (dragging) return;
    let raf;
    const tick = () => {
      const t = Date.now() / 1000;
      yMv.set(baseY.current + Math.sin(t * 0.5  + phase) * 7);
      rotMv.set(anchor.rot  + Math.sin(t * 0.35 + phase) * 1.4);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [dragging, phase, anchor.rot]);

  // Peek: slide outward to reveal image, slide back on second double-click.
  useEffect(() => {
    fmAnimate(xMv, baseX.current + (peekMode ? peekDir : 0),
      { type: "spring", stiffness: 180, damping: 22 });
  }, [peekMode]);

  return (
    <motion.div
      className="sticky-note"
      style={{
        position: "absolute", left: "50%", top: "50%",
        marginLeft: -112, marginTop: -88,
        x: xMv, y: yMv, rotate: rotMv,
        background: color,
        zIndex: dragging ? 200 : 30 + index,
      }}
      initial={{ scale: 0, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      exit={{ scale: 0, opacity: 0, transition: { duration: 0.18 } }}
      transition={{ type: "spring", stiffness: 360, damping: 24, delay: index * 0.07 }}
      drag
      dragMomentum={false}
      onDragStart={() => setDragging(true)}
      onDragEnd={() => {
        // Store drag position relative to the non-peeked baseline.
        baseX.current = xMv.get() - (peekMode ? peekDir : 0);
        baseY.current = yMv.get();
        setDragging(false);
      }}
      whileDrag={{ scale: 1.06 }}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="sn-header">
        <span className="sn-grip" />
        {!href && (
          <button className="sn-del" onClick={(e) => { e.stopPropagation(); onDelete(card.id); }}>
            <IconClose />
          </button>
        )}
      </div>
      {href ? (
        <div className="sn-body sn-source" title={href}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); openUrl(href); }}>
          <span className="sn-source-label">Source</span>
          <span className="sn-source-url">{hrefDomain}</span>
        </div>
      ) : (
        <textarea
          className="sn-body"
          value={card.text}
          onChange={(e) => onTextChange(card.id, e.target.value)}
          placeholder="Note…"
          onPointerDown={(e) => e.stopPropagation()}
          autoFocus={!card.text}
        />
      )}
    </motion.div>
  );
}

function NoteStack({ cards, onPop }) {
  const preview = cards.slice(0, 3);
  return (
    <motion.div
      className="note-stack"
      // Use margin for offset so Framer Motion's transform doesn't conflict with CSS transform
      style={{ position: "absolute", left: "50%", top: "50%", marginLeft: -40, marginTop: 230 }}
      initial={{ opacity: 0, scale: 0.85 }} animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.85 }}
      transition={{ type: "spring", stiffness: 320, damping: 28 }}
      onClick={(e) => { e.stopPropagation(); onPop(); }}
      title={`${cards.length} more note${cards.length > 1 ? "s" : ""} — click to bring forward`}
    >
      {preview.map((_, i) => (
        <div key={i} className="note-stack-card"
          style={{ transform: `rotate(${(i - 1) * 7}deg) translateY(${(2 - i) * 3}px)`, zIndex: 3 - i }} />
      ))}
      <span className="note-stack-count">+{cards.length}</span>
    </motion.div>
  );
}

// Convert a rich clipboard HTML fragment into clean Markdown — headings, lists,
// bold/italic, links, blockquotes, code. Stored as the item's text so both the
// on-disk .md file and the in-app renderMarkdown view look tidy.
function htmlToMarkdown(html) {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.querySelectorAll("script, style, noscript, head, meta, link").forEach(el => el.remove());

  const inline = (node) => Array.from(node.childNodes).map(walk).join("");

  const walk = (node) => {
    if (node.nodeType === Node.TEXT_NODE) return node.textContent.replace(/\s+/g, " ");
    if (node.nodeType !== Node.ELEMENT_NODE) return "";
    const tag = node.tagName.toLowerCase();
    switch (tag) {
      case "h1": return `\n# ${inline(node).trim()}\n\n`;
      case "h2": return `\n## ${inline(node).trim()}\n\n`;
      case "h3": return `\n### ${inline(node).trim()}\n\n`;
      case "h4": case "h5": case "h6": return `\n### ${inline(node).trim()}\n\n`;
      case "strong": case "b": { const t = inline(node).trim(); return t ? `**${t}**` : ""; }
      case "em": case "i": { const t = inline(node).trim(); return t ? `*${t}*` : ""; }
      case "code": return node.closest("pre") ? inline(node) : `\`${inline(node).trim()}\``;
      case "pre": return `\n\`\`\`\n${node.textContent.trim()}\n\`\`\`\n\n`;
      case "br": return "\n";
      case "img": {
        // Only keep absolute http(s) images (relative/data URLs won't render).
        const src = node.getAttribute("src") || "";
        const alt = (node.getAttribute("alt") || "").trim();
        return /^https?:\/\//i.test(src) ? `\n\n![${alt}](${src})\n\n` : "";
      }
      case "figure": return inline(node);
      case "figcaption": { const t = inline(node).trim(); return t ? `\n*${t}*\n\n` : ""; }
      case "a": {
        const t = inline(node).trim(); const href = node.getAttribute("href");
        return href && t && !href.startsWith("javascript:") ? `[${t}](${href})` : t;
      }
      case "blockquote":
        return inline(node).trim().split("\n").map(l => `> ${l}`).join("\n") + "\n\n";
      case "ul": case "ol": {
        const ordered = tag === "ol";
        const items = Array.from(node.children).filter(c => c.tagName.toLowerCase() === "li");
        return "\n" + items.map((li, i) =>
          `${ordered ? `${i + 1}.` : "-"} ${inline(li).trim()}`).join("\n") + "\n\n";
      }
      case "p": case "div": case "section": case "article": {
        const t = inline(node).trim();
        return t ? `${t}\n\n` : "";
      }
      default: return inline(node);
    }
  };

  return walk(doc.body)
    .replace(/\n{3,}/g, "\n\n")   // collapse extra blank lines
    .replace(/[ \t]+\n/g, "\n")   // trim trailing spaces
    .trim();
}

function sanitizeHtml(html) {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.querySelectorAll("script, style, link, meta, head, iframe, object, embed, form, input, button").forEach(el => el.remove());
  doc.querySelectorAll("*").forEach(el => {
    Array.from(el.attributes).forEach(attr => {
      if (attr.name.startsWith("on") || attr.name === "style" || attr.name === "class" || attr.name === "id") {
        el.removeAttribute(attr.name);
      }
    });
    if (el.tagName === "A") { el.removeAttribute("href"); el.removeAttribute("target"); }
  });
  return doc.body.innerHTML;
}

function parseInline(text) {
  const parts = [];
  // image ![alt](src) | link [text](url) | **bold** | *italic* | `code`
  const re = /(!\[([^\]]*)\]\(([^)]+)\)|\[([^\]]+)\]\(([^)]+)\)|\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`)/g;
  let last = 0, m;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    if (m[3] !== undefined) parts.push(<img key={m.index} className="content-inline-img" src={m[3]} alt={m[2]} loading="lazy" />);
    else if (m[4] !== undefined) parts.push(
      <a key={m.index} onClick={(e) => { e.stopPropagation(); openUrl(m[5]); }}>{m[4]}</a>);
    else if (m[6] !== undefined) parts.push(<strong key={m.index}>{m[6]}</strong>);
    else if (m[7] !== undefined) parts.push(<em key={m.index}>{m[7]}</em>);
    else if (m[8] !== undefined) parts.push(<code key={m.index}>{m[8]}</code>);
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts.length === 1 && typeof parts[0] === "string" ? parts[0] : parts;
}

function renderMarkdown(text) {
  if (!text) return null;
  return text.trim().split(/\n{2,}/).map((block, i) => {
    // Code fence
    if (/^```/.test(block)) {
      return <pre key={i}><code>{block.replace(/^```[^\n]*\n?/, "").replace(/\n?```$/, "")}</code></pre>;
    }
    const lines = block.split("\n");
    // Standalone image
    if (lines.length === 1) {
      const im = lines[0].match(/^!\[([^\]]*)\]\(([^)]+)\)$/);
      if (im) return <img key={i} className="content-md-img" src={im[2]} alt={im[1]} loading="lazy" />;
    }
    // Heading (single line)
    if (lines.length === 1) {
      const m = lines[0].match(/^(#{1,3})\s+(.+)/);
      if (m) {
        const level = m[1].length;
        const Tag = `h${level}`;
        return <Tag key={i} className={`md-h${level}`}>{parseInline(m[2])}</Tag>;
      }
    }
    // Ordered list
    if (lines.length > 0 && lines.every(l => /^\d+\.\s/.test(l))) {
      return (
        <ol key={i} className="md-ul">
          {lines.map((l, j) => <li key={j}>{parseInline(l.replace(/^\d+\.\s+/, ""))}</li>)}
        </ol>
      );
    }
    // Bullet list
    if (lines.length > 0 && lines.every(l => /^[-*]\s/.test(l))) {
      return (
        <ul key={i} className="md-ul">
          {lines.map((l, j) => <li key={j}>{parseInline(l.replace(/^[-*]\s+/, ""))}</li>)}
        </ul>
      );
    }
    // Blockquote
    if (lines.every(l => /^>\s?/.test(l))) {
      return (
        <blockquote key={i} className="md-blockquote">
          {parseInline(lines.map(l => l.replace(/^>\s?/, "")).join(" "))}
        </blockquote>
      );
    }
    // Paragraph — join wrapped lines with a space
    return <p key={i} className="md-p">{parseInline(lines.join(" "))}</p>;
  });
}

function DetailModal({ item, onClose, onDelete, onSaved }) {
  // Any item with an image path goes through the image viewer — text/title become sticky notes.
  const isImageOnly = item.type === "image"
    || (item.type === "file" && isImagePath(item.file_path))
    || !!item.image_path;
  const imgPath = isImageOnly ? (item.image_path || item.file_path) : null;
  const isFileItem = item.type === "file" && !isImageOnly;
  const isLinkItem = item.type === "link" && !!item.url && !isImageOnly;

  const [noteCards, setNoteCards] = useState(() => parseNoteCards(item.notes));
  const [img, setImg]             = useState(null);
  const [footerBg, setFooterBg]   = useState(null);       // cropped bottom strip data-url
  const [footerTheme, setFooterTheme] = useState("dark"); // "dark"=white icons, "light"=dark

  // Strip empty cards before serialising — they disappear on next open.
  const toSave = (cards) => serializeNoteCards(cards.filter((c) => c.text.trim()));

  const unmountRef = useRef(null);
  unmountRef.current = { id: item.id, notes: toSave(noteCards) };

  useEffect(() => {
    setNoteCards(parseNoteCards(item.notes));
    setImg(null);
    setFooterBg(null);
    setFooterTheme("dark");
    if (imgPath) invoke("cmd_read_image", { path: imgPath }).then(setImg).catch(console.error);
    const onKey = (e) => {
      if (e.key === "Escape") {
        const { id, notes: n } = unmountRef.current;
        invoke("cmd_update_notes", { id, notes: n }).catch(console.error);
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [item.id]);

  useEffect(() => {
    return () => {
      const { id, notes: n } = unmountRef.current;
      invoke("cmd_update_notes", { id, notes: n }).catch(console.error);
    };
  }, []);

  // Analyse the bottom strip of the loaded image to pick white vs dark footer text.
  useEffect(() => {
    if (!img) return;
    try {
      const image = new Image();
      image.onload = () => {
        try {
          const sampleH = Math.min(80, image.height);
          const canvas = document.createElement("canvas");
          canvas.width = image.width; canvas.height = sampleH;
          const ctx = canvas.getContext("2d");
          ctx.drawImage(image, 0, image.height - sampleH, image.width, sampleH, 0, 0, image.width, sampleH);
          const data = ctx.getImageData(0, 0, image.width, sampleH).data;
          let sum = 0;
          for (let i = 0; i < data.length; i += 4)
            sum += data[i] * 0.299 + data[i + 1] * 0.587 + data[i + 2] * 0.114;
          setFooterTheme(sum / (data.length / 4) > 130 ? "light" : "dark");
          if (isImageOnly) setFooterBg(canvas.toDataURL("image/jpeg", 0.9));
        } catch (_) {}
      };
      image.src = img;
    } catch (_) {}
  }, [img]);

  const saveTimer = useRef(null);
  useEffect(() => {
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      invoke("cmd_update_notes", { id: item.id, notes: toSave(noteCards) }).catch(console.error);
    }, 1200);
    return () => clearTimeout(saveTimer.current);
  }, [noteCards]);

  const addNoteCard = () => {
    if (noteCards.some((c) => !c.text.trim())) return;
    setNoteCards((p) => [...p, { id: crypto.randomUUID(), text: "" }]);
  };
  const updateNoteCard = (id, text) => setNoteCards((p) => p.map((c) => c.id === id ? { ...c, text } : c));
  const deleteNoteCard = (id)       => setNoteCards((p) => p.filter((c) => c.id !== id));

  const flushSave = () => {
    const { id, notes: n } = unmountRef.current;
    invoke("cmd_update_notes", { id, notes: n }).then(onSaved).catch(console.error);
  };
  const closeAndSave = () => { flushSave(); onClose(); };

  const spring = { type: "spring", stiffness: 320, damping: 30 };
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [peekMode, setPeekMode]       = useState(false);

  // Shared footer — textColor and footerClass adapt per context.
  const detailsStrip = (extraDetails, textColor = "var(--text3)", footerClass = "") => (
    <div className={`imgview-foot${footerClass ? " " + footerClass : ""}`}>
      {footerBg && <div className="imgview-foot-bg" style={{ backgroundImage: `url(${footerBg})` }} />}
      <div style={{ flex: 1, position: "relative", height: 36, overflow: "hidden" }}>
        <motion.button className="icon-btn" title="Show details"
          style={{ position: "absolute", left: 0, top: 0, bottom: 0 }}
          animate={{ opacity: detailsOpen ? 0 : 1, x: detailsOpen ? 30 : 0, pointerEvents: detailsOpen ? "none" : "auto" }}
          initial={false}
          transition={{ duration: 0.26, ease: [0.16, 1, 0.3, 1], delay: detailsOpen ? 0 : 0.202 }}
          onClick={() => setDetailsOpen(true)}>
          <IconChevRR />
        </motion.button>
        <motion.div
          style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", gap: 10, paddingLeft: 4, overflow: "hidden", whiteSpace: "nowrap", fontSize: 12, color: textColor }}
          animate={{ clipPath: detailsOpen ? "inset(0 0% 0 0)" : "inset(0 100% 0 0)" }}
          initial={false}
          transition={{ duration: 0.46, ease: [0.16, 1, 0.3, 1], delay: detailsOpen ? 0.1 : 0 }}>
          <span style={{ flexShrink: 0, whiteSpace: "nowrap" }}>Saved {new Date(item.created_at).toLocaleString()}</span>
          {extraDetails}
        </motion.div>
      </div>
      {/* Close-details button lives beside "Add note" (not inside the scrolling
          panel), so its circle never gets clipped. Mirrors the open-chevron's
          motion: on reveal it waits for the panel, then slides into place
          (x:30→0); on close it slides out and collapses immediately. */}
      <AnimatePresence>
        {detailsOpen && (
          <motion.button key="close-details" className="icon-btn"
            style={{ flexShrink: 0, overflow: "hidden" }}
            initial={{ opacity: 0, width: 0, x: -30 }}
            animate={{ opacity: 1, width: 36, x: 0,
              transition: { duration: 0.26, ease: [0.16, 1, 0.3, 1], delay: 0.2 } }}
            exit={{ opacity: 0, width: 0, x: -30,
              transition: { duration: 0.24, ease: [0.16, 1, 0.3, 1] } }}
            onClick={() => setDetailsOpen(false)}>
            <IconChevLL />
          </motion.button>
        )}
      </AnimatePresence>
      <button className={`sn-add-note-btn${noteCards.length > 0 ? " has-notes" : ""}`}
        onClick={addNoteCard}>
        <IconNote />
        <span>Add note</span>
      </button>
      <button className="icon-btn imgview-del" onClick={() => onDelete(item.id)} aria-label="Delete"><IconTrash /></button>
    </div>
  );

  // ── Shared scrim wrapper with floating sticky notes ────────────────────────
  const hasSource = !!(item.url && item.type !== "file");
  const stickyLayer = (
    <AnimatePresence>
      {/* Source card rendered first at a STABLE index 0 so it owns that slot;
          user notes are offset by one so a freshly added note never spawns
          underneath it. */}
      {hasSource && (
        <StickyNote key="__source__" card={{ id: "__source__", text: item.url }}
          index={0} href={item.url} peekMode={peekMode} />
      )}
      {noteCards.map((card, i) => (
        <StickyNote key={card.id} card={card} index={hasSource ? i + 1 : i}
          onTextChange={updateNoteCard} onDelete={deleteNoteCard}
          peekMode={peekMode} />
      ))}
    </AnimatePresence>
  );

  if (isImageOnly) {
    return (
      <motion.div className="scrim" onClick={closeAndSave}
        initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.16 }}>
        {stickyLayer}
        <motion.div className="modal modal-imgview"
          onClick={(e) => e.stopPropagation()}
          onDoubleClick={(e) => { e.stopPropagation(); setPeekMode((v) => !v); }}
          initial={{ opacity: 0, scale: 0.95 }} animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.96 }} transition={spring}>
          <div className="imgview-wrap">
            {img ? <img src={img} alt="" /> : <div className="imgview-placeholder" />}
          </div>
          {detailsStrip(
            <span className="imgview-finder" style={{ flexShrink: 0 }} onClick={() => invoke("cmd_reveal_in_finder", { path: imgPath })}>Show in Finder</span>,
            footerTheme === "dark" ? "rgba(255,255,255,0.72)" : "rgba(0,0,0,0.6)",
            footerTheme === "light" ? "footer-light" : ""
          )}
        </motion.div>
      </motion.div>
    );
  }

  // ── Content modal (highlight / link / text) ────────────────────────────────
  return (
    <motion.div className="scrim" onClick={closeAndSave}
      initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.16 }}>
      {stickyLayer}
      <motion.div className="modal modal-content" onClick={(e) => e.stopPropagation()}
        onDoubleClick={(e) => { e.stopPropagation(); setPeekMode((v) => !v); }}
        initial={{ opacity: 0, scale: 0.95 }} animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.96 }} transition={spring}>
        <div className="content-main">
          {isLinkItem ? (() => {
            const li = linkInfo(item.url);
            return (
              <div className="linkview" onClick={() => openUrl(item.url)}>
                <div className="linkview-banner"
                  style={{ background: `linear-gradient(135deg, hsl(${li.hue} 62% 58%), hsl(${(li.hue + 38) % 360} 60% 46%))` }}>
                  <div className="linkview-fav">
                    <span className="linkview-letter">{li.letter}</span>
                  </div>
                </div>
                <div className="linkview-meta">
                  <div className="linkview-domain">{item.title || li.domain}</div>
                  <div className="linkview-url">{li.domain}{li.path ? li.path.slice(0, 60) + (li.path.length > 60 ? "…" : "") : ""}</div>
                  <span className="linkview-open">Open link ↗</span>
                </div>
              </div>
            );
          })() : isFileItem ? (() => {
            const fk = fileKind(item.file_path || item.title);
            return (
              <div className="fileview" onClick={() => invoke("cmd_open_path", { path: item.file_path })}>
                <div className="fileview-glyph">
                  {fk.cat === "audio" && <span className="fileview-wave" aria-hidden />}
                  <fk.Icon />
                </div>
                <div className="fileview-name">{item.title || baseName(item.file_path) || "File"}</div>
                <span className="fileview-open">Open ↗</span>
              </div>
            );
          })() : (
            <>
              {img && <img className="content-img" src={img} alt="" />}
              {item.title && <div className="content-title">{item.title}</div>}
              {item.html
                ? <div className="content-body" dangerouslySetInnerHTML={{ __html: sanitizeHtml(item.html) }} />
                : item.text && <div className="content-body">{renderMarkdown(item.text)}</div>
              }
            </>
          )}
        </div>
        {detailsStrip(<>
          {item.remind_at && <span style={{ flexShrink: 0 }}>· Reminder {new Date(item.remind_at).toLocaleString()}</span>}
          {item.url && item.type !== "file" && (
            // "Source" link — full URL in the tooltip, click opens the exact URL.
            <span className="imgview-finder" title={item.url} style={{ flexShrink: 0 }}
              onClick={() => openUrl(item.url)}>Source</span>
          )}
          {item.file_path && <span className="imgview-finder" style={{ flexShrink: 0 }} onClick={() => invoke("cmd_open_path", { path: item.file_path })}>Open</span>}
          {item.file_path && <span className="imgview-finder" style={{ flexShrink: 0 }} onClick={() => invoke("cmd_reveal_in_finder", { path: item.file_path })}>Show in Finder</span>}
        </>)}
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
  const [remindersOn, setRemindersOn] = useState(false);

  useEffect(() => {
    invoke("cmd_get_hotkey").then(setHotkey).catch(console.error);
    invoke("cmd_get_storage_dir").then(setStorageDir).catch(console.error);
    invoke("cmd_get_reminders_enabled").then(setRemindersOn).catch(console.error);
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
    const combo = parts.join("+");
    setHotkey(combo);
    setListening(false);
    // Auto-save the moment a new combo is recorded — no Save button needed.
    invoke("cmd_set_hotkey", { hotkey: combo })
      .then(() => { setSaved(true); setTimeout(() => setSaved(false), 1600); })
      .catch(console.error);
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

  const toggleReminders = async () => {
    const next = !remindersOn;
    try {
      await invoke("cmd_set_reminders_enabled", { enabled: next });
      setRemindersOn(next);
      if (next) invoke("cmd_open_notification_settings").catch(console.error);
    } catch (e) { console.error(e); }
  };

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
            <div className="row-label">Shortcut</div>
            <div className="row-sub">Open Shiro from anywhere.</div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div className={`hotkey-badge ${listening ? "listening" : ""}`} tabIndex={0}
              onClick={() => { setListening(true); setSaved(false); }}
              onFocus={() => { setListening(true); setSaved(false); }}
              onBlur={() => setListening(false)}>
              {listening ? "Press keys…" : shortcutDisplay(hotkey)}
            </div>
            {saved && <span className="saved">✓ Saved</span>}
          </div>
        </div>
        <div className="hint">Click it, then press your keys.</div>

        <div className="sec-label">Storage</div>
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="row-label">Saved to</div>
            <div className="row-sub" style={{ wordBreak: "break-all" }} title={storageDir}>{storageDir || "…"}</div>
          </div>
          <button className="btn btn-ghost" onClick={changeLocation} disabled={moving} style={{ whiteSpace: "nowrap" }}>
            {moving ? "Moving…" : "Change…"}
          </button>
        </div>
        <div className="hint">Changing the loaction will move older files as well.</div>

        <div className="sec-label">Reminders</div>
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="row-label">Reminders</div>
            <div className="row-sub">Show a notification when a saved item is due.</div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span className={remindersOn ? "status-ok" : "status-no"}>{remindersOn ? "✓ On" : "Off"}</span>
            <button className={`btn ${remindersOn ? "btn-ghost" : "btn-primary"}`} onClick={toggleReminders}>
              {remindersOn ? "Turn off" : "Turn on"}
            </button>
          </div>
        </div>
        <div className="hint">
          Also allow Shiro in <span className="linklike" onClick={() => invoke("cmd_open_notification_settings")}>System Settings → Notifications</span>.
        </div>

        <div className="sec-label">Permissions</div>
        <div className="row">
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="row-label">Screen access</div>
            <div className="row-sub">Lets Shiro read selected text and take screenshots.</div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span className={axTrusted ? "status-ok" : "status-no"}>{axTrusted ? "✓ Granted" : "Not granted"}</span>
            {!axTrusted && <button className="btn btn-primary" onClick={enableAx}>Enable…</button>}
          </div>
        </div>
        <div className="hint">
          Switch <strong>Shiro</strong> on in <span className="linklike" onClick={() => invoke("cmd_open_accessibility_settings")}>System Settings</span>.
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
  const [remindOpen, setRemindOpen] = useState(false);
  const [remindAt, setRemindAt] = useState("");
  const [noteOpen, setNoteOpen] = useState(false);
  const [remindersEnabled, setRemindersEnabled] = useState(false);
  const noteRef = useRef(null);
  const wrapRef = useRef(null);
  const barRef = useRef(null);
  const stackRef = useRef(null);
  // Tracks `shown` synchronously so the document listener never reads a stale closure.
  const shownRef = useRef(false);

  const reset = () => {
    shownRef.current = false; setShown(false);
    setData({ url: null, title: null, text: null, files: [] });
    setNote(""); setShot(null); setRemindOpen(false); setRemindAt(""); setNoteOpen(false); setSaving(false);
  };

  const requestClose = useCallback((restoreFocus = true) => {
    shownRef.current = false; setShown(false);
    setTimeout(() => invoke("cmd_close_popup", { restoreFocus }), 150);
  }, []);

  useEffect(() => {
    const uns = [];
    listen("popup-data", (e) => {
      // Show the empty bar instantly (0ms), then populate content in the next
      // paint — decouples "pill appears" from "content fills in".
      shownRef.current = true; setShown(true);
      setNote(""); setSaving(false); setRemindOpen(false); setRemindAt(""); setNoteOpen(false);
      setShot(null); setData({ url: null, title: null, text: null, files: [] });
      requestAnimationFrame(() => {
        setData({ files: [], ...e.payload });
        setShot(e.payload.screenshot ? { path: e.payload.screenshot_path, dataUrl: e.payload.screenshot } : null);
        setTimeout(() => wrapRef.current?.focus(), 0);
      });
    }).then((u) => uns.push(u));
    listen("popup-reset", reset).then((u) => uns.push(u));
    listen("popup-dismiss", () => requestClose(false)).then((u) => uns.push(u));
    return () => uns.forEach((u) => u?.());
  }, [requestClose]);

  // Dismiss when clicking outside the visual pill (bar + stack content).
  // pill-wrap is a full-width transparent rectangle — we intentionally ignore its
  // bounds and only treat .bar and .stack as the "inside" hit area.
  useEffect(() => {
    const onMouseDown = (e) => {
      if (!shownRef.current) return;
      if (barRef.current?.contains(e.target) || stackRef.current?.contains(e.target)) return;
      requestClose(true);
    };
    document.addEventListener("mousedown", onMouseDown);
    return () => document.removeEventListener("mousedown", onMouseDown);
  }, [requestClose]);

  useEffect(() => {
    invoke("cmd_get_reminders_enabled").then(setRemindersEnabled).catch(console.error);
  }, []);

  const attachScreenshot = async () => {
    try { const s = await invoke("cmd_take_screenshot"); if (s) setShot({ path: s.path, dataUrl: s.dataUrl }); }
    catch (e) { console.error(e); }
  };

  const handleSave = async () => {
    if (saving) return;
    const hasFiles = data.files?.length > 0;
    if (!hasFiles && !data.url && !data.text?.trim() && !shot && !note.trim()) return;
    setSaving(true);
    const remindStr = remindAt.includes("T") && remindAt.split("T")[1]
      ? remindAt : remindAt ? remindAt.split("T")[0] + "T12:00" : "";
    const remind = remindStr ? new Date(remindStr).toISOString() : null;
    try {
      if (hasFiles) {
        await invoke("cmd_save_files", { paths: data.files, notes: note || null });
      } else {
        const type = data.text?.trim() ? "highlight" : data.url ? "link" : "highlight";
        // Prefer clean Markdown converted from the rich clipboard HTML; fall back
        // to the plain selection. Stored as `text` so the .md file and the in-app
        // view both render formatted. We no longer persist raw HTML.
        let mdText = data.text ?? null;
        // Only treat it as rich HTML if it actually contains tags — guards against
        // raw clipboard dumps (e.g. AppleScript «data …») leaking through.
        if (data.html?.trim() && /<[a-z][\s\S]*>/i.test(data.html)) {
          try { const md = htmlToMarkdown(data.html); if (md.trim()) mdText = md; }
          catch (err) { console.error("html→md failed", err); }
        }
        await invoke("cmd_save_item", {
          req: { type, url: data.url ?? null, title: data.title ?? null, text: mdText, html: null, file_path: null, notes: note || null, remind_at: remind },
          screenshotPath: shot?.path ?? null,
        });
      }
      playCaptureTone();
      requestClose(true);
    } catch (e) { console.error("save failed", e); setSaving(false); }
  };

  const handleKeyDown = (e) => {
    if (e.key === "Escape") requestClose(true);
    if (e.key === "Enter" && (e.metaKey || e.target.tagName === "INPUT")) { e.preventDefault(); handleSave(); }
  };

  const hasText = !!(data.text && data.text.trim());
  const hasContent = data.files?.length > 0 || !!data.url || hasText || !!shot || !!note.trim();
  const showStack = hasContent || noteOpen || remindOpen;
  const isFiles = data.files?.length > 0;

  return (
    <div
      className="pill-wrap"
      ref={wrapRef}
      tabIndex={-1}
      onKeyDown={handleKeyDown}
      style={{
        opacity: shown ? 1 : 0,
        transform: shown ? "translateY(0)" : "translateY(4px)",
        transition: "opacity .09s var(--ease), transform .09s var(--ease)",
        pointerEvents: shown ? "auto" : "none",
      }}
    >
      <AnimatePresence initial={false}>
        {showStack && (
          <motion.div key="stack" className="stack" ref={stackRef}
            initial={{ opacity: 0, y: 10, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.98 }}
            transition={{ duration: 0.26, ease: [0.32, 0, 0.18, 1] }}>
            {/* Screenshot at the top so content/note/alarm are always visible above the bar */}
            {shot && (
              <div className="chip-card shot">
                <img src={shot.dataUrl} alt="screenshot" />
                <button className="shot-x" onClick={() => setShot(null)} aria-label="Remove screenshot"><IconClose /></button>
              </div>
            )}
            {/* Files card — note section folds in at the bottom */}
            {isFiles && (
              <div className="chip-card">
                <div className="ct-files">📎 {data.files.length === 1 ? baseName(data.files[0]) : `${data.files.length} files`}</div>
                {(noteOpen || note) && (
                  <div className="ct-section">
                    <input ref={noteRef} placeholder="Add a note…"
                      value={note} onChange={(e) => setNote(e.target.value)} />
                  </div>
                )}
              </div>
            )}
            {/* Content + note + reminder — one unified card with hairline dividers */}
            {!isFiles && (data.title || data.url || hasText || noteOpen || !!note || remindOpen) && (
              <div className="chip-card">
                {(data.title || hasText) && (
                  <>
                    {data.title && <div className="ct-title">{data.title}</div>}
                    {hasText && <div className="ct-text">{data.text}</div>}
                  </>
                )}
                {(noteOpen || !!note) && (
                  <div className="ct-section">
                    <input ref={noteRef} placeholder="Add a note…"
                      value={note} onChange={(e) => setNote(e.target.value)} />
                  </div>
                )}
                {remindOpen && remindersEnabled && (
                  <div className="ct-section">
                    <div className="ct-section-label">Remind at</div>
                    <div className="ct-remind-row">
                      <input type="date"
                        value={remindAt.split("T")[0] || ""}
                        onChange={(e) => setRemindAt(e.target.value + "T" + (remindAt.split("T")[1] || "12:00"))} />
                      <input type="time"
                        value={remindAt.split("T")[1] || ""}
                        onChange={(e) => setRemindAt((remindAt.split("T")[0] || new Date().toISOString().split("T")[0]) + "T" + e.target.value)} />
                    </div>
                  </div>
                )}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      <div className="bar" ref={barRef}>
        <img className="bar-logo" src={shiroLogo} alt="Shiro" />
        {!isFiles && (
          <button className={`icon-btn ${shot ? "active" : ""}`} onClick={attachScreenshot} title="Screenshot"><IconCamera /></button>
        )}
        <button className={`icon-btn ${noteOpen || note ? "active" : ""}`}
          onClick={() => { const next = !noteOpen; setNoteOpen(next); if (next) setTimeout(() => noteRef.current?.focus(), 0); }} title="Note"><IconNote /></button>
        {!isFiles && hasContent && remindersEnabled && (
          <button className={`icon-btn ${remindOpen || remindAt ? "active" : ""}`} onClick={() => setRemindOpen((v) => !v)} title="Reminder"><IconAlarm /></button>
        )}
        <AnimatePresence initial={false}>
          {hasContent && (
            <motion.div key="save-group"
              style={{ display: "flex", alignItems: "center", gap: 4, overflow: "hidden", whiteSpace: "nowrap" }}
              initial={{ opacity: 0, maxWidth: 0 }}
              animate={{ opacity: 1, maxWidth: 160 }}
              exit={{ opacity: 0, maxWidth: 0 }}
              transition={{ maxWidth: { duration: 0.28, ease: [0.25, 0.46, 0.45, 0.94] }, opacity: { duration: 0.18, ease: "easeOut" } }}>
              <span style={{ minWidth: 4 }} />
              <button className="btn btn-primary save-btn" onClick={handleSave} disabled={saving}>{saving ? "…" : "Save ↵"}</button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

/* ── Save sound ────────────────────────────────────────────────────────────── */
// C5 → E5 → G5 ascending major triad. Two detuned sine waves per note
// create a bell shimmer. ~1 second total, volume 0.07 (subtle).
function playCaptureTone() {
  try {
    const ctx = new (window.AudioContext || window.webkitAudioContext)();
    [[523.25, 0], [659.25, 0.13], [783.99, 0.26]].forEach(([freq, delay]) => {
      [0, 2.5].forEach((detune) => {
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.connect(gain);
        gain.connect(ctx.destination);
        osc.type = "sine";
        osc.frequency.value = freq + detune;
        const t = ctx.currentTime + delay;
        gain.gain.setValueAtTime(0, t);
        gain.gain.linearRampToValueAtTime(0.07, t + 0.008);
        gain.gain.exponentialRampToValueAtTime(0.001, t + 0.9);
        osc.start(t);
        osc.stop(t + 0.9);
      });
    });
  } catch (_) {}
}

/* ── Utils ─────────────────────────────────────────────────────────────────── */
function shortcutDisplay(str) {
  if (!str) return "";
  const m = { Meta: "⌘", Shift: "⇧", Alt: "⌥", Control: "⌃", Ctrl: "⌃" };
  return str.split("+").map((p) => (m[p] || (p.startsWith("Key") ? p.slice(3) : p.startsWith("Digit") ? p.slice(5) : p))).join("");
}
function baseName(path) { return path?.split("/").pop() || path || ""; }
// Always display a URL as just its domain (e.g. "techcrunch.com") — never the full raw string.
function prettyUrl(url) {
  if (!url) return "";
  try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return url; }
}
// Parse a URL into the pieces a link preview needs, with a deterministic hue
// (from the domain) for the fallback gradient tile.
function linkInfo(url) {
  let domain = url || "", path = "";
  try {
    const u = new URL(url);
    domain = u.hostname.replace(/^www\./, "");
    path = (u.pathname + u.search).replace(/\/+$/, "");
  } catch {}
  let h = 0;
  for (let i = 0; i < domain.length; i++) h = (h * 31 + domain.charCodeAt(i)) % 360;
  return { domain, path: path && path !== "" ? path : "", letter: (domain[0] || "?").toUpperCase(), hue: h };
}
function isImagePath(path) {
  if (!path) return false;
  const ext = path.split(".").pop()?.toLowerCase();
  return ["png", "jpg", "jpeg", "gif", "webp", "avif"].includes(ext);
}
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
