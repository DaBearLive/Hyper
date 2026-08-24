<script lang="ts">
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { PhysicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
  import { onMount, tick } from "svelte";
  import { fade, fly } from "svelte/transition";

  let link = $state("");
  let fetching = $state(false);
  let fetched = $state(false); // plain on launch - no preview until Fetch
  let selectedQuality = $state("1080p");
  let savePath = $state("Loading...");
  let errorMsg = $state("");
  let fetchedInfo: any = $state(null);
  let downloading = $state(false);
  let progress = $state(0);
  let view = $state<"fetch" | "library">("fetch");
  let activeFilter = $state("All");
  let sortBy = $state("Newest First");
  let listView = $state<"list" | "tiles">("list");
  let searchQuery = $state("");
  let showSearch = $state(false);

  function isTauri() {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }
  async function minimize() { if (isTauri()) try { await invoke("minimize_window"); } catch { try { await getCurrentWindow().minimize(); } catch {} } }
  async function closeWin() { if (isTauri()) try { await invoke("close_window"); } catch { try { await getCurrentWindow().close(); } catch {} } }
  async function toggleMax() { if (isTauri()) try { await getCurrentWindow().toggleMaximize(); } catch {} }

  let dragStartMouse = $state<{x:number,y:number}|null>(null);
  let dragStartPos = $state<{x:number,y:number}|null>(null);
  async function onDragStart(e: MouseEvent) {
    if (!isTauri()) return;
    try { await invoke("start_dragging"); return; } catch {}
    try { await getCurrentWindow().startDragging(); return; } catch {}
    try {
      const pos = await getCurrentWindow().outerPosition();
      dragStartMouse = { x: e.screenX, y: e.screenY };
      dragStartPos = { x: pos.x, y: pos.y };
      window.addEventListener("mousemove", onDragMove);
      window.addEventListener("mouseup", onDragEnd);
    } catch {}
  }
  async function onDragMove(e: MouseEvent) {
    if (!dragStartMouse || !dragStartPos) return;
    const dx = e.screenX - dragStartMouse.x;
    const dy = e.screenY - dragStartMouse.y;
    try { await getCurrentWindow().setPosition(new PhysicalPosition(dragStartPos.x + dx, dragStartPos.y + dy)); } catch {}
  }
  function onDragEnd() {
    dragStartMouse = null; dragStartPos = null;
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragEnd);
  }
  function addToLibrary() {
    let size = qualities.find(q => q.id === selectedQuality)?.size ?? "0 MB";
    let meta = selectedQuality === "audio" ? "Audio (MP3)" : selectedQuality;
    if (smartMode !== "off" && smartResult) {
      meta = `Smart · ${smartResult.label}${smartResult.transcode ? " → compressed" : ""}`;
      size = fmtEst(smartResult.estBytes);
    }
    const item: Item = {
      id: Date.now(),
      title: fetchedInfo?.title ?? link,
      platform: fetchedInfo?.extractor ?? "YouTube",
      thumb: fetchedInfo?.thumbnail ?? "https://images.unsplash.com/photo-1513407030348-c983a97b98d8?q=80&w=400&auto=format&fit=crop",
      duration: fetchedInfo?.duration ?? "0:00",
      meta, size, date: "Today", quality: selectedQuality, url: link, path: ""
    };
    library = [item, ...library];
  }
  async function refreshLibrary() {
    if (!isTauri()) return;
    try {
      const files = await invoke<any[]>("list_downloads");
      library = files.map((f: any) => ({
        id: f.id,
        title: f.title,
        platform: f.platform,
        thumb: f.thumb || "https://images.unsplash.com/photo-1498049794561-7780e7231661?q=80&w=400&auto=format&fit=crop",
        duration: f.duration || "",
        meta: f.meta,
        size: f.size,
        date: f.date,
        quality: f.quality,
        url: f.url || "",
        path: f.path || ""
      }));
    } catch {}
  }
  function reDownload(item: Item) {
    if (!item.url) {
      errorMsg = "No URL saved for this file — download again to save the link.";
      view = "fetch";
      openMenuId = null;
      return;
    }
    link = item.url;
    fetched = false;
    fetchedInfo = null;
    errorMsg = "";
    view = "fetch";
    openMenuId = null;
  }
  function parentDir(p: string): string {
    const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return i > 0 ? p.slice(0, i) : p;
  }
  function playItem(item: Item) {
    openMenuId = null;
    if (item.path) invoke("open_downloads_folder", { path: item.path }).catch(() => {});
  }
  function revealItem(item: Item) {
    openMenuId = null;
    if (item.path) invoke("open_downloads_folder", { path: parentDir(item.path) }).catch(() => {});
  }
  async function copyLink(item: Item) {
    if (!item.url) return;
    try { await navigator.clipboard.writeText(item.url); }
    catch {
      const ta = document.createElement("textarea");
      ta.value = item.url;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    copiedId = item.id;
    setTimeout(() => { copiedId = null; }, 1400);
  }
  async function deleteItem(item: Item) {
    if (deleteArmId !== item.id) {
      deleteArmId = item.id;
      setTimeout(() => { if (deleteArmId === item.id) deleteArmId = null; }, 3000);
      return;
    }
    deleteArmId = null;
    openMenuId = null;
    try {
      await invoke("delete_download", { path: item.path });
      await refreshLibrary();
    } catch {}
  }

  onMount(async () => {
    if (!isTauri()) {
      savePath = "/home/benl/Downloads/HyperDownloads";
      document.getElementById('hyper-splash')?.remove();
      return;
    }
    try {
      const p = await invoke<string>("get_default_save_path");
      savePath = p;
    } catch {
      savePath = "/home/benl/Downloads/HyperDownloads";
    }
    // system theme listener
    try {
      const m = window.matchMedia("(prefers-color-scheme: dark)");
      systemIsDark = m.matches;
      m.addEventListener("change", e => systemIsDark = e.matches);
    } catch {}
    // restore settings before the persistence effect arms
    try {
      const s = await invoke<any>("get_settings");
      if (["off", "best", "max"].includes(s?.smartMode)) smartMode = s.smartMode;
      if (typeof s?.smartMaxVal === "string" && s.smartMaxVal) smartMaxVal = s.smartMaxVal;
      if (s?.smartUnit === "GB") smartUnit = "GB";
      if (typeof s?.savePath === "string" && s.savePath) savePath = s.savePath;
      if (typeof s?.accentColor === "string" && /^#[0-9a-fA-F]{6}$/.test(s.accentColor)) accentColor = s.accentColor;
      if (["dark","light","system"].includes(s?.theme)) theme = s.theme;
    } catch {}
    settingsLoaded = true;
    await refreshLibrary();
    document.getElementById('hyper-splash')?.remove();
    if (isTauri()) {
      try { await getCurrentWindow().show(); } catch {}
      try { await getCurrentWindow().setFocus(); } catch {}
      await tick();
      await syncWindowSize(false);
    }
  });

  // persist Smart Quality settings on change
  $effect(() => { persistSmart(); });

  // return to compact when link cleared
  $effect(() => {
    if (link.trim() === "" && fetched) {
      fetched = false;
      fetchedInfo = null;
    }
  });

  // adaptive window: resize when content changes
  $effect(() => {
    void fetched;
    void errorMsg;
    void view;
    void smartMode;
    void showSmartDetails;
    void smartResult;
    void downloading;
    void showSettings;
    void filtered.length;
    void fetching;
    tick().then(() => syncWindowSize(true));
  });

  // refresh when switching to library
  $effect(() => {
    if (view === "library") {
      refreshLibrary();
    }
  });

  type Quality = { label: string; sub?: string; format: string; size: string; id: string };
  const baseQualities: Quality[] = [
    { id: "1080p", label: "1080p", sub: "(Best)", format: "MP4", size: "128 MB" },
    { id: "720p", label: "720p", format: "MP4", size: "64 MB" },
    { id: "480p", label: "480p", format: "MP4", size: "32 MB" },
    { id: "360p", label: "360p", format: "MP4", size: "18 MB" },
    { id: "audio", label: "Audio Only", sub: "(MP3)", format: "MP3", size: "6 MB" },
  ];
  let showSettings = $state(false);
  let outerEl: HTMLDivElement | null = null;
  let cardEl: HTMLDivElement | null = null;
  let settingsEl: HTMLDivElement | null = null;
  // Appearance
  let accentColor = $state("#38bdf8");
  let theme = $state<"dark" | "light" | "system">("dark");
  let systemIsDark = $state(true);
  const effectiveTheme = $derived(theme === "system" ? (systemIsDark ? "dark" : "light") : theme);
  // derived accent variants
  function hexToRgb(hex: string) {
    const h = hex.replace("#", "");
    const full = h.length === 3 ? h.split("").map(c => c + c).join("") : h;
    const n = parseInt(full, 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }
  function adjustHex(hex: string, amt: number) {
    const { r, g, b } = hexToRgb(hex);
    const c = (v: number) => Math.max(0, Math.min(255, Math.round(v + amt)));
    return `rgb(${c(r)},${c(g)},${c(b)})`;
  }
  const accentHover = $derived(adjustHex(accentColor, -22));
  const accentLight = $derived(effectiveTheme === "light" ? adjustHex(accentColor, -14) : adjustHex(accentColor, 70));
  const accentHeading = $derived(effectiveTheme === "light" ? adjustHex(accentColor, -32) : adjustHex(accentColor, 70));
  const accentRgb = $derived.by(() => { const { r, g, b } = hexToRgb(accentColor); return `${r},${g},${b}`; });
  const accentBg = $derived(`rgba(${accentRgb},0.15)`);
  const accentBorder = $derived(`rgba(${accentRgb},0.38)`);
  const themeVars = $derived.by(() => {
    const accentA06 = `rgba(${accentRgb},0.07)`;
    const accentA04 = `rgba(${accentRgb},0.04)`;
    return effectiveTheme === "light"
      ? {
          bgCard: "#ffffff",
          bgCardGradient: `linear-gradient(180deg, #ffffff 0%, #f8fafc 100%)`,
          cardShadow: "0 1px 3px rgba(148,163,184,0.08), 0 8px 24px rgba(148,163,184,0.14), 0 4px 12px rgba(148,163,184,0.08)",
          bgPage: "#f1f5f9",
          bgPageGradient: `linear-gradient(135deg, #f1f5f9 0%, ${accentA06} 55%, #e8eefc 100%)`,
          bgInner: "#eef2f7",
          bgInnerGradient: `linear-gradient(180deg, #f1f5f9 0%, #eef2f7 100%)`,
          bgElevated: "#e2e8f0",
          bgTitle: "#ffffff",
          borderCard: "#cbd5e1",
          borderInner: "#cbd5e1",
          textPri: "#0f172a",
          textSec: "#475569",
          textMut: "#64748b"
        }
      : {
          bgCard: "#121214",
          bgCardGradient: "linear-gradient(180deg, #1e1e22 0%, #121214 100%)",
          cardShadow: "0 8px 28px rgba(0,0,0,0.45), 0 2px 8px rgba(0,0,0,0.35)",
          bgPage: "#0e0e10",
          bgPageGradient: "linear-gradient(180deg, #1a1a1e 0%, #0e0e10 100%)",
          bgInner: "#1a1a1e",
          bgInnerGradient: "linear-gradient(180deg, #1e1e22 0%, #1a1a1e 100%)",
          bgElevated: "#1e1e22",
          bgTitle: "#121214",
          borderCard: "#1e1e22",
          borderInner: "#27272e",
          textPri: "#ffffff",
          textSec: "#9ca3af",
          textMut: "#8a8a90"
        };
  });
  let qualitySizes = $state<Record<string, string>>({});
  const qualities = $derived(baseQualities.map(q => ({ ...q, size: qualitySizes[q.id] ?? q.size })));

  // ---- Smart Quality ----
  type SmartMode = "off" | "best" | "max";
  type SmartPick = {
    videoId: string; audioId: string | null;
    label: string; estBytes: number; fits: boolean;
    transcode: boolean;
    breakdown: { label: string; estMB: number | null; score: number }[];
  };
  let smartMode = $state<SmartMode>("off");
  let smartMaxVal = $state("20");
  let smartUnit = $state<"MB" | "GB">("MB");
  let smartError = $state("");
  let showSmartDetails = $state(false);
  let settingsLoaded = $state(false);
  let fetchedFormats: any[] = [];
  let fetchedDuration = 0;

  // Estimated bytes of a single format (exact size -> approx -> bitrate x duration)
  function fmtBytes(f: any, dur: number): number {
    if (f.filesize && f.filesize > 0) return f.filesize;
    if (f.filesize_approx && f.filesize_approx > 0) return f.filesize_approx;
    const tbr = f.tbr ?? ((f.vbr || 0) + (f.abr || 0)) ?? 0;
    return tbr > 0 && dur > 0 ? tbr * 1000 / 8 * dur : 0;
  }
  // Newer codecs hit the same visual quality at lower bitrates -> small preference bonus
  const CODEC_EFF: [string, number][] = [["av01", 1.35], ["av1", 1.35], ["hevc", 1.25], ["h265", 1.25], ["vp9", 1.2], ["vp09", 1.2]];
  function codecEff(f: any): number {
    const v = (f.vcodec || "").toLowerCase();
    for (const [k, m] of CODEC_EFF) if (v.includes(k)) return m;
    return 1;
  }
  // Composite visual-quality score: pixels dominate, fps & codec efficiency nudge it.
  // 1080p30 beats 720p60; same res prefers higher fps.
  function qualityScore(f: any): number {
    const h = f.height ?? 0;
    const w = f.width ?? Math.round(h * 16 / 9);
    if (!w || !h) return 0;
    const fps = Math.min(f.fps ?? 30, 60);
    return w * h * (1 + (fps - 30) / 300) * codecEff(f);
  }
  function fmtLabel(v: any): string {
    const fps = v.fps ? `${Math.round(v.fps)} FPS` : "";
    return `${v.height ?? "?"}p${fps ? " · " + fps : ""}${(v.vcodec || "").toLowerCase().includes("av01") ? " · AV1" : ""}`;
  }
  function smartPick(limitBytes: number | null): SmartPick | null {
    const dur = fetchedDuration;
    const vids = fetchedFormats.filter((f: any) => f.vcodec && f.vcodec !== "none");
    const auds = fetchedFormats.filter((f: any) => f.vcodec === "none" && f.acodec && f.acodec !== "none");
    // Audio-only source (e.g. music post)
    if (!vids.length) {
      const best = auds.sort((a: any, b: any) => fmtBytes(b, dur) - fmtBytes(a, dur))[0];
      if (!best) return null;
      const est = fmtBytes(best, dur);
      return { videoId: best.format_id, audioId: null, label: `Audio · ${(best.abr ?? best.tbr ?? 0).toFixed(0)}kbps`, estBytes: est, fits: limitBytes == null || est <= limitBytes, transcode: false,
        breakdown: [{ label: `Audio ${Math.round(best.abr ?? best.tbr ?? 0)}kbps`, estMB: est / 1048576, score: 1 }] };
    }
    const bestAud = auds.sort((a: any, b: any) => fmtBytes(b, dur) - fmtBytes(a, dur))[0];
    const audB = bestAud ? fmtBytes(bestAud, dur) : 0;
    // Dedupe by resolution+fps, keep highest-scored variant per bucket
    const buckets = new Map<string, { v: any; est: number; score: number }>();
    for (const v of vids) {
      const combined = v.acodec && v.acodec !== "none";
      const est = fmtBytes(v, dur) + (combined ? 0 : audB);
      if (est <= 0) continue;
      const key = `${v.height ?? "?"}x${Math.round(v.fps ?? 0)}`;
      const score = qualityScore(v);
      const prev = buckets.get(key);
      if (!prev || score > prev.score) buckets.set(key, { v, est, score });
    }
    const opts = [...buckets.values()].sort((a, b) => b.score - a.score);
    if (!opts.length) return null;
    const mkBreakdown = () => opts.slice(0, 6).map(o => ({
      label: fmtLabel(o.v), estMB: o.est / 1048576, score: o.score
    }));
    const best = opts[0];
    if (limitBytes == null) {
      return { videoId: best.v.format_id, audioId: bestAud?.format_id ?? null, label: fmtLabel(best.v), estBytes: best.est, fits: true, transcode: false, breakdown: mkBreakdown() };
    }
    const fit = opts.find(o => o.est <= limitBytes);
    if (fit) {
      return { videoId: fit.v.format_id, audioId: bestAud?.format_id ?? null, label: fmtLabel(fit.v), estBytes: fit.est, fits: true, transcode: false, breakdown: mkBreakdown() };
    }
    // Nothing fits as-is: download best source, Rust compresses to the limit afterwards
    return { videoId: best.v.format_id, audioId: bestAud?.format_id ?? null, label: fmtLabel(best.v), estBytes: best.est, fits: false, transcode: true, breakdown: mkBreakdown() };
  }

  const smartLimitBytes = $derived.by(() => {
    const n = parseFloat(smartMaxVal);
    if (!isFinite(n) || n <= 0) return NaN;
    return smartUnit === "GB" ? n * 1024 * 1024 * 1024 : n * 1024 * 1024;
  });
  const smartValid = $derived(!isNaN(smartLimitBytes) && smartLimitBytes >= 1024 * 1024 * 0.5);
  const smartResult = $derived.by(() => {
    if (smartMode === "off" || !fetchedFormats.length) return null;
    if (smartMode === "max") {
      if (!smartValid) return null;
      return smartPick(smartLimitBytes);
    }
    return smartPick(null);
  });
  function fmtEst(bytes: number): string {
    const mb = bytes / 1048576;
    return mb >= 1024 ? `~${(mb / 1024).toFixed(2)} GB` : `~${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  }
  function persistSmart() {
    if (!settingsLoaded || !isTauri()) return;
    invoke("set_settings", { settings: { smartMode, smartMaxVal, smartUnit, savePath, accentColor, theme } }).catch(() => {});
  }
  // keep CSS vars in sync
  $effect(() => {    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.style.setProperty("--accent", accentColor);
    root.style.setProperty("--accent-hover", accentHover);
    root.style.setProperty("--accent-light", accentLight);
    root.style.setProperty("--accent-heading", accentHeading);
    root.style.setProperty("--accent-rgb", accentRgb);
    root.style.setProperty("--accent-bg", accentBg);
    root.style.setProperty("--accent-border", accentBorder);
    // theme vars
    root.style.setProperty("--bg-card", themeVars.bgCard);
    root.style.setProperty("--bg-card-gradient", (themeVars as any).bgCardGradient);
    root.style.setProperty("--card-shadow", (themeVars as any).cardShadow);
    root.style.setProperty("--bg-page", themeVars.bgPage);
    root.style.setProperty("--bg-page-gradient", (themeVars as any).bgPageGradient);
    root.style.setProperty("--bg-inner", themeVars.bgInner);
    root.style.setProperty("--bg-inner-gradient", (themeVars as any).bgInnerGradient);
    root.style.setProperty("--bg-elevated", themeVars.bgElevated);
    root.style.setProperty("--bg-title", themeVars.bgTitle);
    root.style.setProperty("--border-card", themeVars.borderCard);
    root.style.setProperty("--border-inner", themeVars.borderInner);
    root.style.setProperty("--text-pri", themeVars.textPri);
    root.style.setProperty("--text-sec", themeVars.textSec);
    root.style.setProperty("--text-mut", themeVars.textMut);
    root.classList.toggle("theme-light", effectiveTheme === "light");
    root.classList.toggle("theme-dark", effectiveTheme === "dark");
    if (cardEl) {
      cardEl.style.setProperty("--accent", accentColor);
      cardEl.style.setProperty("--accent-hover", accentHover);
      (cardEl.style as any).setProperty("--bg-card-gradient", (themeVars as any).bgCardGradient);
      (cardEl.style as any).setProperty("--card-shadow", (themeVars as any).cardShadow);
    }
  });
  // keep taskbar/app icon bolt in sync with accent colour
  $effect(() => {
    if (!settingsLoaded || !isTauri()) return;
    const hex = accentColor;
    const t = setTimeout(() => { invoke("set_accent_icon", { hex }).catch(() => {}); }, 300);
    return () => clearTimeout(t);
  });

  async function openSettings() {
    showSettings = true;
    await tick();
    try { await invoke("resize_window", { width: 880, height: 680 }); } catch {}
  }
  async function closeSettings() {
    showSettings = false;
    await tick();
    try { await invoke("resize_window", { width: 480, height: 680 }); } catch {}
  }
  async function syncWindowSize(_animate = true) { return; }


  type Item = { id:number; title:string; platform:string; thumb:string; duration:string; meta:string; quality:string; size:string; date:string; url:string; path:string; };
  let library = $state<Item[]>([]);
  let openMenuId = $state<number|null>(null);
  let copiedId = $state<number|null>(null);
  let deleteArmId = $state<number|null>(null);

  let filtered = $derived(
    [...library]
      .filter(i => {
        const q = searchQuery.trim().toLowerCase();
        return q === "" || i.title.toLowerCase().includes(q) || i.platform.toLowerCase().includes(q);
      })
      .sort((a,b) => {
        if (sortBy === "Largest First") {
          const pa = parseFloat(a.size) || 0;
          const pb = parseFloat(b.size) || 0;
          return pb - pa;
        }
        if (sortBy === "Oldest First") return a.id - b.id;
        return b.id - a.id;
      })
  );
  let totalMB = $derived(library.reduce((s, i) => s + (parseFloat(i.size) || 0), 0));
  let totalStr = $derived(totalMB >= 1024 ? `${(totalMB/1024).toFixed(2)} GB` : `${totalMB} MB`);

  async function doFetch() {
    if (!link.trim()) return;
    fetching = true;
    errorMsg = "";
    fetchedInfo = null;
    fetched = false;
    fetchedFormats = [];
    fetchedDuration = 0;
    // web preview without Tauri - use dynamic mock based on link
    if (!isTauri()) {
      await new Promise(r => setTimeout(r, 600));
      let host = "Video";
      let extractor = "YouTube";
      try {
        const u = new URL(link);
        host = u.hostname.replace("www.", "");
        if (host.includes("twitter") || host.includes("x.com")) extractor = "X (Twitter)";
        else if (host.includes("instagram")) extractor = "Instagram";
        else if (host.includes("tiktok")) extractor = "TikTok";
        else if (host.includes("facebook")) extractor = "Facebook";
      } catch {}
      fetchedInfo = {
        title: `Video from ${host} — ${link.slice(0, 50)}${link.length>50?"…":""}`,
        thumbnail: "https://images.unsplash.com/photo-1513407030348-c983a97b98d8?q=80&w=400&auto=format&fit=crop",
        duration: "0:00",
        extractor
      };
      fetched = true;
      fetching = false;
      return;
    }
    try {
      const jsonStr = await invoke<string>("fetch_video_info", { url: link });
      const data = JSON.parse(jsonStr);
      const thumb = data.thumbnail ?? data.thumbnails?.slice(-1)?.[0]?.url ?? data.thumbnails?.[0]?.url ?? "https://images.unsplash.com/photo-1513407030348-c983a97b98d8?q=80&w=400&auto=format&fit=crop";
      // normalize extractor name
      let ext = data.extractor ?? "YouTube";
      if (ext.toLowerCase().includes("twitter")) ext = "X (Twitter)";
      else if (ext.toLowerCase().includes("instagram")) ext = "Instagram";
      else if (ext.toLowerCase().includes("youtube")) ext = "YouTube";
      fetchedInfo = {
        title: data.title ?? link,
        thumbnail: thumb,
        duration: data.duration_string ?? (data.duration ? `${Math.floor(data.duration/60)}:${String(Math.floor(data.duration%60)).padStart(2,'0')}` : "0:00"),
        extractor: ext
      };
      if (data.hyper_sizes && typeof data.hyper_sizes === "object") {
        qualitySizes = data.hyper_sizes;
      } else {
        qualitySizes = {};
      }
      // Smart Quality needs the raw format list
      fetchedFormats = Array.isArray(data.formats) ? data.formats : [];
      fetchedDuration = typeof data.duration === "number" ? data.duration : 0;
      showSmartDetails = false;
      fetched = true;
    } catch (e: any) {
      console.warn("fetch_video_info failed:", e);
      errorMsg = typeof e === 'string' ? e : e?.message ?? String(e);
      // keep compact — do not expand on failure
      fetched = false;
      fetchedInfo = null;
    } finally {
      fetching = false;
    }
  }

  async function pickFolder() {
    if (!isTauri()) return;
    try {
      const selected: any = await open({ directory: true, multiple: false, defaultPath: savePath });
      const picked = Array.isArray(selected) ? selected[0] : selected;
      if (typeof picked === "string" && picked) savePath = picked;
    } catch (e) {
      console.error("pickFolder", e);
    }
  }

  async function openFolder() {
    if (!isTauri()) {
      // in browser preview, just show path
      errorMsg = `Downloads folder: ${savePath}`;
      return;
    }
    try {
      await invoke("open_downloads_folder", { path: savePath });
    } catch {}
  }

  async function doDownload() {
    if (downloading) return;
    if (!link.trim() && !fetched) return;
    // Smart Quality: resolve what we'll actually download before starting
    let qualityParam = selectedQuality;
    let formatOverride: string | null = null;
    let maxBytes: number | null = null;
    if (smartMode === "max") {
      if (!smartValid) { smartError = `Enter a valid size (min 0.5 MB)`; return; }
      if (smartResult) {
        if (smartResult.fits) {
          const spec = [smartResult.videoId, smartResult.audioId].filter(Boolean).join("+");
          formatOverride = `${spec}/best`;
          qualityParam = "smart";
        } else {
          // no format fits: grab best source, Rust compresses to the limit
          qualityParam = "1080p";
          maxBytes = smartLimitBytes;
        }
      } else {
        maxBytes = smartLimitBytes; // no format data available - download best, compress after
      }
    } else if (smartMode === "best" && smartResult) {
      const spec = [smartResult.videoId, smartResult.audioId].filter(Boolean).join("+");
      formatOverride = `${spec}/best`;
      qualityParam = "smart";
    }
    downloading = true;
    progress = 0;
    errorMsg = "";
    if (!isTauri()) {
      const interval = setInterval(() => {
        progress += Math.random()*18 + 8;
        if (progress >= 100) {
          progress = 100;
          clearInterval(interval);
          addToLibrary();
          setTimeout(()=> { downloading=false; progress=0; view="library"; }, 600);
        }
      }, 180);
      return;
    }
    let unlisten: (()=>void)|null = null;
    try {
      // Listen for real yt-dlp progress from Rust
      try {
        unlisten = await listen<number>("download-progress", (e) => {
          const v = typeof e.payload === "number" ? e.payload : parseFloat(String(e.payload));
          if (!isNaN(v)) progress = Math.min(100, Math.max(0, v));
        });
      } catch {}
      const downloadPromise = invoke<string>("download_video", { url: link || "https://www.youtube.com/watch?v=dQw4w9WgXcQ", quality: qualityParam, savePath, ...(formatOverride ? { formatOverride } : {}), ...(maxBytes != null ? { maxBytes } : {}) });

      await downloadPromise;
      if (unlisten) try { unlisten(); } catch {}
      progress = 100;
      await refreshLibrary();
      setTimeout(()=> { downloading=false; progress=0; view="library"; }, 800);
    } catch (e: any) {
      if (unlisten) try { unlisten(); } catch {}
      // fallback mock progress if not in Tauri or yt-dlp fails
      console.warn("download_video failed, using mock:", e);
      errorMsg = typeof e === 'string' ? e : e?.message ?? String(e);
      const interval = setInterval(() => {
        progress += Math.random()*18 + 8;
        if (progress >= 100) {
          progress = 100;
          clearInterval(interval);
          setTimeout(()=> { downloading=false; progress=0; view="library"; }, 600);
        }
      }, 180);
    }
  }
</script>

<svelte:window onclick={()=>{ openMenuId = null; deleteArmId = null; }} />

<svelte:head>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
</svelte:head>

<div bind:this={outerEl} class="w-screen h-screen bg-transparent flex items-center justify-center p-3 gap-3 select-none">
  <!-- Main card -->
  <div bind:this={cardEl} class="w-[480px] shrink-0 bg-[var(--bg-card)] rounded-2xl overflow-hidden flex flex-col max-h-[calc(100vh-24px)] transition-all duration-300 ease-out" style="background: var(--bg-card-gradient, var(--bg-card)); box-shadow: var(--card-shadow);">
    <!-- Title bar - draggable -->
    <div class="h-[52px] flex items-center justify-between px-5 border-b border-[var(--border-card)] shrink-0 bg-[var(--bg-card)]" style="-webkit-app-region: drag">
      <div onmousedown={onDragStart} data-tauri-drag-region class="flex items-center gap-2.5 flex-1 h-full cursor-grab active:cursor-grabbing select-none" style="-webkit-app-region: drag">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"><path d="M13 2L3 14h7l-1 8 10-12h-7l1-8z" fill="var(--accent-light)" stroke="var(--accent-light)" stroke-width="1.2" stroke-linejoin="round"/></svg>
        <span class="text-[17px] font-semibold tracking-tight text-[var(--text-pri)]">Hyper</span>
      </div>
      <div class="flex items-center gap-1 text-[var(--text-mut)]" style="-webkit-app-region: no-drag">
        <button onclick={minimize} style="-webkit-app-region: no-drag" class="w-8 h-8 grid place-items-center rounded-lg hover:bg-[var(--bg-elevated)] hover:text-[var(--text-pri)] transition" aria-label="minimize">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M5 12h14"/></svg>
        </button>
        <button onclick={closeWin} style="-webkit-app-region: no-drag" class="w-8 h-8 grid place-items-center rounded-lg hover:bg-[#23232a] hover:text-[var(--text-pri)] transition" aria-label="close">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M6 6l12 12M18 6L6 18"/></svg>
        </button>
      </div>
    </div>

    {#if view==="fetch"}
      <!-- Fetch View — compact until fetched -->
      <div class="p-3 space-y-3 bg-[var(--bg-page)]" style="background: var(--bg-page-gradient, var(--bg-page));">
        <!-- Input -->
        <div class="bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl p-1.5 flex items-center gap-2">
          <div class="flex items-center gap-3 flex-1 min-w-0 px-3 text-[var(--text-sec)]">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
            <input bind:value={link} placeholder="Paste video link here..." class="bg-transparent outline-none text-[14px] placeholder:text-[var(--text-mut)] text-[var(--text-pri)] flex-1 min-w-0" />
          </div>
          <button onclick={doFetch} disabled={fetching} class="bg-[var(--accent)] hover:bg-[var(--accent-hover)] disabled:opacity-60 text-white text-[14px] font-medium px-[18px] py-[9px] rounded-lg transition shrink-0">
            {fetching ? 'Fetching...' : 'Fetch'}
          </button>
        </div>

        {#if errorMsg}
          <div transition:fade={{ duration: 180 }} class="bg-[#2a1a1a] border border-[#3a2020] text-[#f87171] text-[12px] px-3 py-2 rounded-lg">{errorMsg}</div>
        {/if}

        {#if fetched}
          <!-- Video Card — expands with window (height stays final, only opacity/transform animates so window can measure correctly) -->
          <div in:fly={{ y: 12, duration: 280 }} out:fade={{ duration: 180 }} class="bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl p-2.5 space-y-2.5" style="background: var(--bg-inner-gradient, var(--bg-inner));">
            <div class="flex gap-3">
              <div class="relative w-[132px] h-[80px] rounded-lg overflow-hidden bg-black shrink-0">
                <img src={fetchedInfo?.thumbnail ?? "https://images.unsplash.com/photo-1513407030348-c983a97b98d8?q=80&w=400&auto=format&fit=crop"} alt="thumb" class="w-full h-full object-cover" />
                <span class="absolute bottom-1.5 right-1.5 bg-black/75 text-white text-[11px] font-medium px-1.5 py-0.5 rounded">{fetchedInfo?.duration ?? "3:42"}</span>
              </div>
              <div class="flex-1 min-w-0 pt-1">
                <h3 class="text-[var(--text-pri)] font-medium text-[15px] leading-tight line-clamp-2">{fetchedInfo?.title ?? "Night Walk in Tokyo 4K"}</h3>
                <div class="flex items-center gap-1.5 mt-2">
                  <span class="w-5 h-5 grid place-items-center"><svg width="20" height="14" viewBox="0 0 24 24" fill="none"><path fill="#FF0000" d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814z"/><path fill="white" d="M9.545 15.568V8.432L15.818 12z"/></svg></span>
                  <span class="text-[13px] text-[var(--text-sec)]">YouTube</span>
                </div>
              </div>
            </div>

            <!-- Quality - compact -->
            <div class="bg-[var(--bg-card)] border border-[var(--border-inner)] rounded-xl p-2.5" style="background: var(--bg-card-gradient, var(--bg-card));">
              <div class="flex items-center justify-between mb-2">
                <p class="text-[12px] font-medium text-[var(--text-pri)]">Quality</p>
                <select
                  bind:value={smartMode}
                  onchange={() => { smartError = ""; showSmartDetails = false; }}
                  class="appearance-none bg-[var(--bg-inner)] border border-[var(--border-inner)] text-[var(--accent)] text-[11px] font-medium rounded-lg pl-2 pr-7 py-1 outline-none cursor-pointer hover:border-[var(--accent)]/40 focus:border-[var(--accent)]/50"
                  style="background-image: url(&quot;data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2338bdf8' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E&quot;); background-repeat: no-repeat; background-position: right 6px center;"
                >
                  <option value="off">Standard</option>
                  <option value="best">Smart · Best Available</option>
                  <option value="max">Smart · Max File Size</option>
                </select>
              </div>

              {#if smartMode === "off"}
                <div class="space-y-0.5">
                  {#each qualities as q}
                    <label class="flex items-center justify-between py-1.5 px-1 rounded-lg hover:bg-[var(--bg-elevated)] cursor-pointer group">
                      <div class="flex items-center gap-3">
                        <span class="w-[18px] h-[18px] rounded-full border-2 grid place-items-center {selectedQuality===q.id ? 'border-[var(--accent)] bg-[var(--accent)]/15' : 'border-[#3a3a40] group-hover:border-[#4a4a52]'}">
                          {#if selectedQuality===q.id}<span class="w-[8px] h-[8px] rounded-full bg-[var(--accent)]"></span>{/if}
                        </span>
                        <input type="radio" name="quality" value={q.id} bind:group={selectedQuality} class="hidden" />
                        <span class="text-[14px] {selectedQuality===q.id ? 'text-[var(--text-pri)] font-medium' : 'text-[var(--text-sec)]'}">{q.label} {#if q.sub}<span class="font-normal text-[var(--text-sec)]">{q.sub}</span>{/if}</span>
                      </div>
                      <div class="flex items-center gap-6">
                        <span class="text-[11px] font-semibold tracking-wide px-2 py-1 rounded-md bg-[var(--accent-bg)] text-[var(--accent-light)] border border-[var(--accent)]/40">{q.format}</span>
                        <span class="text-[13px] text-[var(--text-sec)] w-[54px] text-right tabular-nums">{q.size}</span>
                      </div>
                    </label>
                  {/each}
                </div>
              {:else}
                {#if smartMode === "max"}
                  <div class="flex items-center gap-2 mb-2">
                    <span class="text-[12px] text-[var(--text-sec)] shrink-0">Maximum size</span>
                    <input
                      type="text" inputmode="decimal" bind:value={smartMaxVal}
                      oninput={() => { smartError = ""; }}
                      class="w-[72px] bg-[var(--bg-inner)] border border-[var(--border-inner)] focus:border-[var(--accent)] text-[var(--text-pri)] text-[13px] tabular-nums rounded-lg px-2.5 py-1.5 outline-none transition"
                    />
                    <select
                      bind:value={smartUnit}
                      onchange={() => { smartError = ""; }}
                      class="appearance-none bg-[var(--bg-inner)] border border-[var(--border-inner)] text-[var(--accent)] text-[12px] font-medium rounded-lg pl-2 pr-7 py-1.5 outline-none cursor-pointer hover:border-[var(--accent)]/40 focus:border-[var(--accent)]/50"
                      style="background-image: url(&quot;data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2338bdf8' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E&quot;); background-repeat: no-repeat; background-position: right 6px center;"
                    >
                      <option value="MB">MB</option>
                      <option value="GB">GB</option>
                    </select>
                  </div>
                  {#if smartError || (smartMaxVal !== "" && !smartValid)}
                    <p class="text-[11px] text-[#f87171] mb-2">{smartError || "Enter a number ≥ 0.5 — e.g. 5, 20, 100 or 1 GB"}</p>
                  {/if}
                {/if}

                {#if smartResult}
                  {@const pickLabel = smartMode === "max"
                    ? (smartResult.transcode ? `${smartResult.label} → compressed` : smartResult.label)
                    : smartResult.label}
                  <div class="bg-[var(--accent-bg)]/40 border border-[var(--accent)]/30 rounded-lg p-2.5">
                    <div class="flex items-start justify-between gap-2">
                      <div class="min-w-0">
                        <p class="text-[10px] uppercase tracking-wider text-[var(--accent-light)] font-semibold mb-0.5">Selected</p>
                        <p class="text-[14px] text-[var(--text-pri)] font-medium truncate">{pickLabel}</p>
                      </div>
                      <span class="text-[13px] text-[var(--accent-light)] font-semibold tabular-nums whitespace-nowrap">{fmtEst(smartResult.estBytes)}</span>
                    </div>
                    {#if smartMode === "max"}
                      <p class="text-[11px] text-[var(--text-sec)] mt-1">{smartResult.transcode ? `Nothing fits ${smartMaxVal}${smartUnit === "GB" ? " GB" : " MB"} as-is — best source will be re-encoded to fit` : `Fits under your ${smartMaxVal}${smartUnit === "GB" ? " GB" : " MB"} limit`}</p>
                    {/if}
                    {#if smartResult.breakdown.length > 1}
                      <button onclick={() => showSmartDetails = !showSmartDetails} class="mt-1.5 text-[11px] text-[var(--accent-light)] hover:text-[var(--text-pri)] transition flex items-center gap-1">
                        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="transition-transform {showSmartDetails ? 'rotate-90' : ''}"><path d="M9 18l6-6-6-6"/></svg>
                        Why this one?
                      </button>
                      {#if showSmartDetails}
                        <div class="mt-1.5 space-y-0.5">
                          {#each smartResult.breakdown as b}
                            <div class="flex items-center justify-between text-[11px] {b.score === smartResult.breakdown[0].score ? 'text-[var(--text-pri)]' : 'text-[var(--text-mut)]'}">
                              <span>{b.label}</span>
                              <span class="tabular-nums">{b.estMB != null ? `~${b.estMB < 10 ? b.estMB.toFixed(1) : Math.round(b.estMB)} MB` : "?"}</span>
                            </div>
                          {/each}
                          <p class="text-[10px] text-[var(--text-mut)] pt-1">Sizes are estimates from stream metadata; final file may differ slightly.</p>
                        </div>
                      {/if}
                    {/if}
                  </div>
                {:else if smartValid || smartMode === "best"}
                  <p class="text-[12px] text-[var(--text-mut)] italic">Fetch a video to see the smart selection.</p>
                {/if}
              {/if}
            </div>

            <!-- Save to - compact -->
            <div class="flex items-center gap-2">
              <span class="text-[12px] text-[var(--text-sec)] shrink-0">Save to</span>
              <div class="flex-1 bg-[var(--bg-card)] border border-[var(--border-inner)] rounded-lg px-2.5 py-2 flex items-center gap-2 min-w-0">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
                <span class="text-[12px] text-[var(--text-pri)] truncate">{savePath}</span>
              </div>
              <button onclick={pickFolder} class="bg-[var(--bg-elevated)] hover:bg-[var(--bg-elevated)] border border-[var(--border-inner)] text-[var(--text-pri)] text-[12px] font-medium px-3 py-2 rounded-lg transition">Change</button>
            </div>
          </div>
        {/if}
      </div>
      {#if fetched}
        <!-- Download — revealed on expand -->
        <div in:fly={{ y: 10, duration: 280, delay: 60 }} out:fade={{ duration: 160 }} class="shrink-0 p-3 pt-0 bg-[var(--bg-page)]">
          <button onclick={doDownload} disabled={downloading || (smartMode === "max" && !smartValid)} class="relative overflow-hidden w-full font-semibold text-[14px] py-[12px] rounded-xl flex items-center justify-center gap-2 transition border {downloading ? 'bg-[var(--bg-elevated)] border-[var(--border-inner)] text-white cursor-not-allowed' : 'bg-[var(--accent)] hover:bg-[var(--accent-hover)] border-transparent text-white shadow-[0_4px_20px_rgba(56,189,248,0.35)] disabled:opacity-60 disabled:cursor-not-allowed'}">
            {#if downloading}
              <div class="absolute inset-y-0 left-0 bg-[var(--accent)] transition-all duration-200 ease-linear" style="width:{progress}%"></div>
              <span class="relative z-10 flex items-center justify-center gap-2">
                <svg class="animate-spin" width="18" height="18" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="9" stroke="rgba(255,255,255,0.35)" stroke-width="2"/><path d="M21 12a9 9 0 00-9-9" stroke="white" stroke-width="2" stroke-linecap="round"/></svg>
                Downloading {Math.round(progress)}%
              </span>
            {:else}
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              Download
            {/if}
          </button>
        </div>
      {/if}
      <div class="h-[52px] bg-[var(--bg-card)] border-t border-[var(--border-card)] flex items-center justify-between px-4 shrink-0">
        <button onclick={openFolder} class="flex items-center gap-2 text-[13px] text-[var(--text-sec)] hover:text-[var(--text-pri)] transition">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
          Open Downloads
        </button>
        <button onclick={()=>view="library"} class="flex items-center gap-1.5 text-[13px] text-[var(--text-sec)] hover:text-[var(--text-pri)] transition">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
          Downloads
        </button>
      </div>
    {:else}
      <!-- Library View -->
      <div class="flex-1 flex flex-col min-h-0 bg-[var(--bg-page)] relative">
        <!-- Search -->
        <div class="bg-[var(--bg-card)] border-b border-[var(--border-card)] px-3 py-2 flex items-center gap-2">
          <div class="flex-1 bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl px-3 py-2 flex items-center gap-2">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="11" cy="11" r="8"/><path d="M21 21l-4.3-4.3"/></svg>
            <input bind:value={searchQuery} placeholder="Search downloads..." class="flex-1 bg-transparent outline-none text-[13px] text-[var(--text-pri)] placeholder:text-[var(--text-mut)]" />
            {#if searchQuery}
              <button onclick={()=>searchQuery=""} class="w-6 h-6 grid place-items-center text-[var(--text-mut)] hover:text-[var(--text-pri)] rounded-lg transition">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M6 6l12 12M18 6L6 18"/></svg>
              </button>
            {/if}
          </div>
        </div>

        <!-- Toolbar -->
        <div class="bg-[var(--bg-page)] px-3 py-2.5 flex items-center justify-end gap-2 border-b border-[var(--border-inner)]">
          <div class="relative">
            <select bind:value={sortBy} class="appearance-none bg-[var(--bg-inner)] border border-[var(--border-inner)] text-[var(--text-pri)] text-[13px] rounded-lg pl-3 pr-7 py-1.5 outline-none">
              <option>Newest First</option>
              <option>Oldest First</option>
              <option>Largest First</option>
            </select>
            <svg class="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M6 9l6 6 6-6"/></svg>
          </div>
          <div class="flex bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-lg p-1 gap-1">
            <button onclick={()=>listView="list"} class="w-7 h-7 grid place-items-center rounded-md {listView==='list' ? 'bg-[var(--bg-inner)] text-white' : 'text-[var(--text-mut)]'}"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><circle cx="3" cy="6" r="1" fill="currentColor"/><circle cx="3" cy="12" r="1" fill="currentColor"/><circle cx="3" cy="18" r="1" fill="currentColor"/></svg></button>
            <button onclick={()=>listView="tiles"} class="w-7 h-7 grid place-items-center rounded-md {listView==='tiles' ? 'bg-[var(--bg-inner)] text-white' : 'text-[var(--text-mut)]'}"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="4" width="5" height="7" rx="1"/><rect x="9.5" y="4" width="5" height="7" rx="1"/><rect x="16" y="4" width="5" height="7" rx="1"/><rect x="3" y="13" width="5" height="7" rx="1"/><rect x="9.5" y="13" width="5" height="7" rx="1"/><rect x="16" y="13" width="5" height="7" rx="1"/></svg></button>
          </div>
          <button onclick={openSettings} class="w-8 h-8 grid place-items-center bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-lg text-[var(--text-mut)] hover:text-[var(--accent)] hover:border-[var(--accent)]/30 transition" aria-label="Settings">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.77 3.77z"/></svg>
          </button>
        </div>

        <!-- List -->
        <div class="overflow-y-auto p-3 bg-[var(--bg-page)] max-h-[520px]" style="background: var(--bg-page-gradient, var(--bg-page));">
          {#snippet itemMenu(item: Item)}
              <div class="relative self-center mr-1 shrink-0">
                <button onclick={(e)=>{ e.stopPropagation(); openMenuId = openMenuId===item.id ? null : item.id; }} class="w-8 h-8 grid place-items-center text-[var(--text-mut)] hover:text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] rounded-lg transition">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="1.4" fill="currentColor"/><circle cx="12" cy="5" r="1.4" fill="currentColor"/><circle cx="12" cy="19" r="1.4" fill="currentColor"/></svg>
                </button>
                {#if openMenuId===item.id}
                  <div class="absolute right-0 top-9 z-20 w-48 bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl shadow-[0_4px_16px_rgba(0,0,0,0.18)] overflow-hidden py-1" onclick={(e)=>e.stopPropagation()}>
                    {#if item.path}
                      <button onclick={()=>playItem(item)} class="w-full text-left px-3 py-2 text-[13px] text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] flex items-center gap-2 transition">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none"><path d="M6 4l14 8-14 8V4z"/></svg>
                        Play
                      </button>
                      <button onclick={()=>revealItem(item)} class="w-full text-left px-3 py-2 text-[13px] text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] flex items-center gap-2 transition">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
                        Show in Folder
                      </button>
                    {/if}
                    {#if item.url}
                      <button onclick={()=>copyLink(item)} class="w-full text-left px-3 py-2 text-[13px] text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] flex items-center gap-2 transition">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
                        {copiedId===item.id ? "Copied!" : "Copy Link"}
                      </button>
                    {/if}
                    <div class="h-px bg-[var(--border-inner)] mx-2 my-1"></div>
                    <button onclick={()=>reDownload(item)} class="w-full text-left px-3 py-2 text-[13px] text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] flex items-center gap-2 transition">
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                      Re-download
                    </button>
                    {#if item.path}
                      <div class="h-px bg-[var(--border-inner)] mx-2 my-1"></div>
                      <button onclick={()=>deleteItem(item)} class="w-full text-left px-3 py-2 text-[13px] hover:bg-red-500/10 flex items-center gap-2 transition {deleteArmId===item.id ? 'text-red-400 font-medium' : 'text-red-400/90'}">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
                        {deleteArmId===item.id ? "Confirm Delete?" : "Delete"}
                      </button>
                    {/if}
                    {#if !item.url && !item.path}
                      <p class="px-3 pb-1 pt-0.5 text-[11px] text-[var(--text-mut)]">No URL saved for this file</p>
                    {/if}
                  </div>
                {/if}
              </div>
          {/snippet}
          {#if listView === "list"}
          <div class="space-y-3">
          {#each filtered as item}
            <div class="bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl p-2.5 flex gap-3.5 hover:bg-[var(--bg-elevated)] hover:border-[var(--border-inner)] transition group">
              <div class="relative w-[132px] h-[84px] rounded-lg overflow-hidden bg-black shrink-0">
                <img src={item.thumb} alt={item.title} class="w-full h-full object-cover" />
                <span class="absolute bottom-1.5 right-1.5 bg-black/75 text-white text-[11px] font-medium px-1.5 py-0.5 rounded">{item.duration}</span>
              </div>
              <div class="flex-1 min-w-0 py-1 flex flex-col justify-center">
                <h4 class="text-[var(--text-pri)] font-medium text-[14.5px] leading-tight truncate pr-2">{item.title}</h4>
                <div class="flex items-center gap-1.5 mt-1.5">
                  {#if item.platform==="YouTube"}<span class="w-4 h-4 grid place-items-center"><svg width="16" height="11" viewBox="0 0 24 24" fill="none"><path fill="#FF0000" d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814z"/><path fill="white" d="M9.545 15.568V8.432L15.818 12z"/></svg></span>{/if}
                  {#if item.platform==="Instagram"}<span class="w-4 h-4 rounded bg-gradient-to-br from-[#feda75] via-[#d62976] to-[#4f5bd5] grid place-items-center"><svg width="9" height="9" viewBox="0 0 24 24" fill="white"><path d="M7.0301.084c-1.2768.0602-2.1487.264-2.911.5634-.7888.3075-1.4575.72-2.1228 1.3877-.6652.6677-1.075 1.3368-1.3802 2.127-.2954.7638-.4956 1.6365-.552 2.914-.0564 1.2775-.0689 1.6882-.0626 4.947.0062 3.2586.0206 3.6671.0825 4.9473.061 1.2765.264 2.1482.5635 2.9107.308.7889.72 1.4573 1.388 2.1228.6679.6655 1.3365 1.0743 2.1285 1.38.7632.295 1.6361.4961 2.9134.552 1.2773.056 1.6884.069 4.9462.0627 3.2578-.0062 3.668-.0207 4.9478-.0814 1.28-.0607 2.147-.2652 2.9098-.5633.7889-.3086 1.4578-.72 2.1228-1.3881.665-.6682 1.0745-1.3378 1.3795-2.1284.2957-.7632.4966-1.636.552-2.9124.056-1.2809.0692-1.6898.063-4.948-.0063-3.2583-.021-3.6668-.0817-4.9465-.0607-1.2797-.264-2.1487-.5633-2.9117-.3084-.7889-.72-1.4568-1.3876-2.1228C21.2982 1.33 20.628.9208 19.8378.6165 19.074.321 18.2017.1197 16.9244.0645 15.6471.0093 15.236-.005 11.977.0014 8.718.0076 8.31.0215 7.0301.0839m.1402 21.6932c-1.17-.0509-1.8053-.2453-2.2287-.408-.5606-.216-.96-.4771-1.3819-.895-.422-.4178-.6811-.8186-.9-1.378-.1644-.4234-.3624-1.058-.4171-2.228-.0595-1.2645-.072-1.6442-.079-4.848-.007-3.2037.0053-3.583.0607-4.848.05-1.169.2456-1.805.408-2.2282.216-.5613.4762-.96.895-1.3816.4188-.4217.8184-.6814 1.3783-.9003.423-.1651 1.0575-.3614 2.227-.4171 1.2655-.06 1.6447-.072 4.848-.079 3.2033-.007 3.5835.005 4.8495.0608 1.169.0508 1.8053.2445 2.228.408.5608.216.96.4754 1.3816.895.4217.4194.6816.8176.9005 1.3787.1653.4217.3617 1.056.4169 2.2263.0602 1.2655.0739 1.645.0796 4.848.0058 3.203-.0055 3.5834-.061 4.848-.051 1.17-.245 1.8055-.408 2.2294-.216.5604-.4763.96-.8954 1.3814-.419.4215-.8181.6811-1.3783.9-.4224.1649-1.0577.3617-2.2262.4174-1.2656.0595-1.6448.072-4.8493.079-3.2045.007-3.5825-.006-4.848-.0608M16.953 5.5864A1.44 1.44 0 1 0 18.39 4.144a1.44 1.44 0 0 0-1.437 1.4424M5.8385 12.012c.0067 3.4032 2.7706 6.1557 6.173 6.1493 3.4026-.0065 6.157-2.7701 6.1506-6.1733-.0065-3.4032-2.771-6.1565-6.174-6.1498-3.403.0067-6.156 2.771-6.1496 6.1738M8 12.0077a4 4 0 1 1 4.008 3.9921A3.9996 3.9996 0 0 1 8 12.0077"/></svg></span>{/if}
                  {#if item.platform==="X (Twitter)"}<span class="w-4 h-4 rounded bg-black border border-[var(--border-inner)] grid place-items-center"><svg width="9" height="9" viewBox="0 0 24 24" fill="white"><path d="M14.234 10.162 22.977 0h-2.072l-7.591 8.824L7.251 0H.258l9.168 13.343L.258 24H2.33l8.016-9.318L16.749 24h6.993zm-2.837 3.299-.929-1.329L3.076 1.56h3.182l5.965 8.532.929 1.329 7.754 11.09h-3.182z"/></svg></span>{/if}
                  <span class="text-[13px] text-[var(--text-sec)]">{item.platform}</span>
                </div>
                <div class="flex items-center gap-1.5 mt-1 text-[12.5px] text-[var(--text-sec)]">
                  <span>{item.meta}</span><span class="w-1 h-1 rounded-full bg-[#4b5563]"></span><span>{item.size}</span><span class="w-1 h-1 rounded-full bg-[#4b5563]"></span><span>{item.date}</span>
                </div>
              </div>
              {@render itemMenu(item)}
            </div>
          {/each}
          </div>
          {:else}
          <div class="grid grid-cols-3 gap-2.5">
          {#each filtered as item}
            <div class="relative bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl hover:border-[var(--accent)]/30 transition group flex flex-col">
              <div class="w-full aspect-video bg-black rounded-t-xl overflow-hidden shrink-0">
                <img src={item.thumb} alt={item.title} class="w-full h-full object-cover" />
                <span class="absolute bottom-1 right-1 bg-black/75 text-white text-[9px] font-medium px-1 py-0.5 rounded">{item.duration}</span>
              </div>
              <div class="absolute top-1 right-1 opacity-0 group-hover:opacity-100 transition z-10">
                {@render itemMenu(item)}
              </div>
              <div class="p-2 flex flex-col gap-0.5 flex-1">
                <h4 class="text-[11.5px] font-medium text-[var(--text-pri)] leading-snug line-clamp-2">{item.title}</h4>
                <p class="text-[10px] text-[var(--text-mut)] mt-auto truncate">{item.meta} • {item.size}</p>
              </div>
            </div>
          {/each}
          </div>
          {/if}
          {#if filtered.length===0}
            <p class="text-center text-sm text-[var(--text-mut)] py-10">No items for {activeFilter}</p>
          {/if}
        </div>

        <!-- Footer with Back -->
        <div class="h-[52px] bg-[var(--bg-card)] border-t border-[var(--border-card)] flex items-center justify-between px-3 shrink-0 gap-2">
          <button onclick={()=>view="fetch"} class="flex items-center gap-1.5 px-2.5 py-1.5 text-[13px] font-medium text-[var(--text-sec)] hover:text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] rounded-lg transition shrink-0">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M19 12H5M12 19l-7-7 7-7"/></svg>
            Back
          </button>
          <span class="text-[12px] text-[var(--text-sec)] hidden sm:block">{filtered.length} items • {totalStr}</span>
          <button onclick={openFolder} class="flex items-center gap-1.5 text-[12px] sm:text-[13px] font-medium text-[var(--accent)] hover:text-[var(--accent-light)] transition shrink-0">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
            <span class="hidden sm:inline">Open Downloads Folder</span><span class="sm:hidden">Open</span>
          </button>
        </div>

      </div>
    {/if}

  </div>
  {#if showSettings}
    <div bind:this={settingsEl} in:fly={{ x: 20, duration: 250 }} class="w-[360px] shrink-0 bg-[var(--bg-inner)] border border-[var(--border-inner)] rounded-xl overflow-hidden shadow-none flex flex-col max-h-[calc(100vh-24px)]" style="background: var(--bg-inner-gradient, var(--bg-inner));">
        <div class="flex items-center justify-between px-4 py-3 border-b border-[var(--border-inner)] shrink-0">
          <h3 class="text-[var(--text-pri)] font-semibold text-[14px] flex items-center gap-2">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--accent)" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.77 3.77z"/></svg>
            Settings
          </h3>
          <button onclick={closeSettings} class="w-7 h-7 grid place-items-center text-[var(--text-mut)] hover:text-[var(--text-pri)] hover:bg-[var(--bg-elevated)] rounded-lg transition">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M6 6l12 12M18 6L6 18"/></svg>
          </button>
        </div>
        <div class="p-4 space-y-5 overflow-y-auto">
          <!-- Appearance -->
          <div>
            <p class="text-[11px] font-semibold tracking-wider text-[var(--accent-heading)] uppercase mb-2">Appearance</p>
            <div class="space-y-3">
              <div class="flex items-center justify-between gap-3">
                <span class="text-[12px] text-[var(--text-sec)]">Theme</span>
                <select
                  bind:value={theme}
                  class="appearance-none bg-[var(--bg-card)] border border-[var(--border-inner)] text-[var(--accent)] text-[12px] font-medium rounded-lg pl-2 pr-7 py-1.5 outline-none cursor-pointer hover:border-[var(--accent)]/30"
                  style="background-image: url(&quot;data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2338bdf8' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E&quot;); background-repeat: no-repeat; background-position: right 6px center;"
                >
                  <option value="dark">Dark</option>
                  <option value="light">Light</option>
                  <option value="system">System</option>
                </select>
              </div>
              <div>
                <div class="flex items-center justify-between">
                  <span class="text-[12px] text-[var(--text-sec)]">Accent colour</span>
                  <button onclick={()=>accentColor='#38bdf8'} class="text-[11px] text-[var(--accent)] hover:underline">Reset</button>
                </div>
                <div class="flex items-center gap-2 mt-1.5">
                  <input type="color" bind:value={accentColor} class="w-9 h-9 rounded-lg border border-[var(--border-inner)] p-1 bg-[var(--bg-card)] cursor-pointer shrink-0" />
                  <span class="text-[12px] font-mono text-[var(--text-sec)] select-all">{accentColor}</span>
                </div>
                <div class="flex gap-1.5 mt-2 flex-wrap">
                  {#each ["#38bdf8","#a78bfa","#f472b6","#34d399","#f59e0b","#ef4444","#22c55e","#0ea5e9"] as c}
                    <button onclick={()=>accentColor=c} aria-label={c} class="w-7 h-7 rounded-full border-2 transition {accentColor.toLowerCase()===c ? 'border-[var(--text-pri)] scale-110' : 'border-[var(--border-inner)] hover:scale-105'}" style="background-color:{c}"></button>
                  {/each}
                </div>
              </div>
            </div>
          </div>
          <!-- Save location -->
          <div>
            <p class="text-[11px] font-semibold tracking-wider text-[var(--accent-heading)] uppercase mb-2">Save Location</p>
            <div class="bg-[var(--bg-card)] border border-[var(--border-inner)] rounded-lg px-2.5 py-2 flex items-center gap-2 min-w-0">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" class="shrink-0"><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/></svg>
              <span class="text-[12px] text-[var(--text-pri)] truncate flex-1">{savePath}</span>
            </div>
            <div class="flex gap-2 mt-2">
              <button onclick={pickFolder} class="flex-1 bg-[var(--bg-elevated)] hover:bg-[var(--bg-elevated)] border border-[var(--border-inner)] text-[var(--text-pri)] text-[12px] font-medium py-1.5 rounded-lg transition">Change</button>
              <button onclick={openFolder} class="flex-1 bg-[var(--accent-bg)] hover:bg-[var(--accent-bg)]/80 border border-[var(--accent)]/30 text-[var(--accent-heading)] text-[12px] font-medium py-1.5 rounded-lg transition">Open Folder</button>
            </div>
          </div>
          <!-- Smart Quality default -->
          <div>
            <p class="text-[11px] font-semibold tracking-wider text-[var(--accent-heading)] uppercase mb-2">Smart Quality</p>
            <p class="text-[11px] text-[var(--text-sec)] mb-2">Default maximum size when Smart · Max File Size is selected.</p>
            <div class="flex items-center gap-2">
              <input
                type="text" inputmode="decimal" bind:value={smartMaxVal}
                class="w-[72px] bg-[var(--bg-card)] border border-[var(--border-inner)] focus:border-[var(--accent)] text-[var(--text-pri)] text-[13px] tabular-nums rounded-lg px-2.5 py-1.5 outline-none transition"
              />
              <select
                bind:value={smartUnit}
                class="appearance-none bg-[var(--bg-card)] border border-[var(--border-inner)] text-[var(--accent)] text-[12px] font-medium rounded-lg pl-2 pr-7 py-1.5 outline-none cursor-pointer hover:border-[var(--accent)]/30 focus:border-[var(--accent)]/50"
                style="background-image: url(&quot;data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2338bdf8' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E&quot;); background-repeat: no-repeat; background-position: right 6px center;"
              >
                <option value="MB">MB</option>
                <option value="GB">GB</option>
              </select>
              {#if smartMaxVal !== "" && !smartValid}
                <span class="text-[11px] text-[#f87171]">≥ 0.5 required</span>
              {/if}
            </div>
          </div>
          <!-- About -->
          <div class="pt-2 border-t border-[var(--border-inner)]">
            <p class="text-[11px] font-semibold tracking-wider text-[var(--accent-heading)] uppercase mb-2">About</p>
            <div class="bg-[var(--bg-card)] border border-[var(--border-inner)] rounded-xl p-3 flex items-start gap-3">
              <div class="w-10 h-10 rounded-lg grid place-items-center shrink-0" style="background:#0e0e10;">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none"><path d="M13 2L3 14h7l-1 8 10-12h-7l1-8z" fill="var(--accent)" stroke="var(--accent-light)" stroke-width="1" stroke-linejoin="round"/></svg>
              </div>
              <div class="min-w-0">
                <p class="text-[13.5px] font-semibold text-[var(--text-pri)] leading-tight flex items-center gap-1.5">
                  Hyper
                  <span class="text-[10px] font-semibold text-[var(--accent)] bg-[var(--accent-bg)] border border-[var(--accent)]/30 rounded px-1 py-px tracking-wide">v1.0.0</span>
                </p>
                <p class="text-[11.5px] text-[var(--text-sec)] mt-1">Made by <span class="text-[var(--text-pri)] font-medium">Ben Lampard</span></p>
                <p class="text-[11px] text-[var(--text-mut)] mt-0.5">Designed for Linux &amp; Windows</p>
              </div>
            </div>
            <p class="text-[10.5px] text-[var(--text-mut)] text-center pt-2">{filtered.length} items • {totalStr}</p>
          </div>
        </div>
      </div>
  {/if}
</div>

<style>
  /* svelte style placeholder - keeps postcss happy */
  :global(html) { scrollbar-width: thin; }
</style>
