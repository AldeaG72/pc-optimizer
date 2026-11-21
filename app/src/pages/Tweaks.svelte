<script lang="ts">
  import BadgeCheck from "@lucide/svelte/icons/badge-check";
  import Gauge from "@lucide/svelte/icons/gauge";
  import Search from "@lucide/svelte/icons/search";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import X from "@lucide/svelte/icons/x";
  import type { Category, Recipe, StateView } from "../lib/backend";
  import { prefs, t, ui } from "../lib/state.svelte";
  import {
    catalog,
    clearPending,
    loadCatalog,
    pending,
    pendingCount,
    reapply,
    recommended,
    switchEnabled,
    switchOn,
    tickRecommended,
    toggle,
  } from "../lib/tweaks.svelte";

  type View = Category | "recommended";
  const ORDER: Category[] = [
    "power", "gaming", "input", "windows", "network", "audio",
    "debloat", "privacy", "cleanup", "services", "advanced",
  ];

  let view = $state<View>("recommended");
  let query = $state("");
  let selectedId = $state<string | null>(null);

  $effect(() => {
    void loadCatalog(ui.lang);
  });

  // "Fix" on Home, or "See the recommendations".
  $effect(() => {
    const focus = ui.focus;
    if (!focus || catalog.list.length === 0) return;
    if (focus === "recommended") view = "recommended";
    else {
      const r = catalog.list.find((x) => x.id === focus);
      if (r) {
        view = r.category;
        selectedId = r.id;
      }
    }
    ui.focus = null;
  });

  const visible = $derived(
    catalog.list.filter((r) => (r.category !== "advanced" || prefs.showAdvanced) && !r.category.startsWith("gpu-")),
  );
  const categories = $derived(ORDER.filter((c) => visible.some((r) => r.category === c)));
  const recommendedList = $derived(recommended(visible, catalog.laptop));

  const rows = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q) {
      return visible.filter((r) =>
        [r.name, r.description, r.id].some((s) => s.toLowerCase().includes(q)),
      );
    }
    return view === "recommended" ? recommendedList : visible.filter((r) => r.category === view);
  });
  const selected = $derived(selectedId ? visible.find((r) => r.id === selectedId) ?? null : null);

  const catKey = (c: View) => c.replace("-", "_");
  const count = (c: View) =>
    c === "recommended" ? recommendedList.length : visible.filter((r) => r.category === c).length;

  function tag(r: Recipe): { key: string; tone: string } | null {
    if (pending.apply.includes(r.id)) return { key: "app.tag.apply", tone: "accent" };
    if (pending.undo.includes(r.id)) return { key: "app.tag.undo", tone: "accent" };
    if (r.status.code === "modified_outside") return { key: "app.tag.outside", tone: "amber" };
    if (r.status.code === "already_in_place") return { key: "app.tag.in_place", tone: "neutral" };
    if (r.status.code === "not_applicable") return { key: "app.tag.not_applicable", tone: "neutral" };
    return null;
  }

  function stateText(s: StateView): string {
    if (s.kind === "missing") return t("app.detail.missing");
    if (s.kind === "value") return s.text;
    return t(`app.service.${s.start}`);
  }

  function statusNote(r: Recipe): string | null {
    const s = r.status;
    if (s.code === "not_applicable") {
      const reason = s.reason;
      if (reason.code === "service_missing") return t("app.na.service_missing", { name: reason.name });
      if (reason.code === "precondition") return t("app.na.precondition", { target: reason.target });
      return t("app.na.platform");
    }
    if (s.code === "error") return t("app.status.error", { message: s.message });
    if (s.code === "already_in_place") return t("app.detail.in_place");
    return null;
  }

  function apply() {
    ui.page = "apply";
  }
</script>

<div class="tweaks">
  <div class="main">
    <div class="top">
      <h1>{t("app.tweaks.title")}</h1>
      <label class="search">
        <Search size={15} aria-hidden="true" />
        <span class="sr">{t("app.tweaks.search")}</span>
        <input type="search" placeholder={t("app.tweaks.search")} bind:value={query} />
      </label>
    </div>

    {#if catalog.error}
      <div class="card note error" role="alert">{t("app.tweaks.error", { message: catalog.error })}</div>
    {:else if catalog.list.length === 0}
      <p class="muted">{t("app.tweaks.loading")}</p>
    {:else}
      <div class="cols">
        <nav class="cats" aria-label={t("app.tweaks.categories")}>
          {#each ["recommended" as View, ...categories] as c (c)}
            <button class:on={!query && view === c} aria-current={!query && view === c ? "true" : undefined} onclick={() => { query = ""; view = c; }}>
              <span>{t(`app.cat.${catKey(c)}`)}</span>
              <span class="n">{count(c)}</span>
            </button>
          {/each}
        </nav>

        <div class="list-col">
          {#if !query}
            <div class="list-head">
              <div>
                <h2>{t(`app.cat.${catKey(view)}`)}</h2>
                <div class="muted small">{t(`app.catdesc.${catKey(view)}`)}</div>
              </div>
              <button class="btn" title={t("app.tweaks.recommended_hint")} onclick={() => tickRecommended(view === "recommended" ? null : view)}>
                <BadgeCheck size={15} aria-hidden="true" />
                {t("app.tweaks.recommended")}
              </button>
            </div>
          {/if}

          {#if rows.length === 0}
            <div class="card note muted">{query ? t("app.tweaks.no_match", { query }) : t("app.tweaks.none")}</div>
          {:else}
            <ul class="card list">
              {#each rows as r (r.id)}
                {@const tg = tag(r)}
                <li class:selected={selectedId === r.id}>
                  <button class="row-main" onclick={() => (selectedId = r.id)} aria-label={r.name}>
                    <span class="name">{r.name}</span>
                    <span class="meta">
                      <span class="chip {r.risk}">{t(`app.risk.${r.risk}`)}</span>
                      <span class="muted small">{r.impact}</span>
                      {#if tg}<span class="chip tag {tg.tone}">{t(tg.key)}</span>{/if}
                    </span>
                  </button>
                  <button
                    class="switch"
                    class:on={switchOn(r)}
                    role="switch"
                    aria-checked={switchOn(r)}
                    aria-label={r.name}
                    disabled={!switchEnabled(r)}
                    onclick={() => toggle(r)}
                  ><span></span></button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <aside class="detail" aria-labelledby="detail-title">
    {#if selected}
      {@const note = statusNote(selected)}
      <div class="d-head">
        <h2 id="detail-title">{selected.name}</h2>
        <button class="icon-btn" aria-label={t("app.detail.close")} onclick={() => (selectedId = null)}><X size={15} /></button>
      </div>
      <div class="meta">
        <span class="chip {selected.risk}">{t(`app.risk.${selected.risk}`)}</span>
        <span class="muted small">{selected.impact}</span>
      </div>
      {#if note}<div class="callout">{note}</div>{/if}
      {#if selected.status.code === "modified_outside"}
        <div class="callout amber">
          {t("app.detail.outside")}
          <button class="btn" onclick={() => reapply(selected)}>{t("app.detail.reapply")}</button>
        </div>
      {/if}
      <div><div class="label">{t("app.detail.what")}</div>{selected.description}</div>
      <div><div class="label">{t("app.detail.why")}</div>{selected.why}</div>
      <div><div class="label">{t("app.detail.breaks")}</div>{selected.breaks}</div>
      {#if selected.restart !== "none"}<div class="muted small">{t(`app.detail.restart_${selected.restart}`)}</div>{/if}
      {#if selected.needsAdmin}<div class="muted small admin"><ShieldCheck size={13} aria-hidden="true" /> {t("app.detail.admin")}</div>{/if}
      {#if selected.changes.length}
        <details open>
          <summary class="label">{t("app.detail.technical")}</summary>
          <div class="code">
            {#each selected.changes as c, i (i)}
              <div class="change">
                <div>{c.target}</div>
                <div>{stateText(c.current)} → {stateText(c.desired)}</div>
              </div>
            {/each}
          </div>
        </details>
      {/if}
      {#if selected.sources.length}
        <div>
          <div class="label">{t("app.detail.sources")}</div>
          {#each selected.sources as s (s)}<div class="source">{s}</div>{/each}
        </div>
      {/if}
    {:else}
      <p class="muted">{t("app.detail.select")}</p>
    {/if}
  </aside>

  {#if pendingCount() > 0}
    <div class="bar" role="region" aria-label={t("app.bar.apply")}>
      <div>
        <strong>{t("app.bar.count", { count: pendingCount() })}</strong>
        <span class="muted">{t("app.bar.note")}</span>
      </div>
      <div class="bar-actions">
        <button class="btn" onclick={clearPending}>{t("app.bar.clear")}</button>
        <button class="btn" onclick={() => { ui.benchFirst = true; apply(); }}>
          <Gauge size={15} aria-hidden="true" />{t("app.bar.measure")}
        </button>
        <button class="btn primary" onclick={() => { ui.benchFirst = false; apply(); }}>
          <ShieldCheck size={15} aria-hidden="true" />{t("app.bar.apply")}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .tweaks {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 290px;
    grid-template-rows: minmax(0, 1fr) auto;
    height: calc(100% + 48px);
    margin: -24px -28px;
  }
  .main {
    padding: 22px 22px 16px 26px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    overflow: auto;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    width: 240px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
  }
  .search input {
    border: 0;
    outline: 0;
    background: transparent;
    width: 100%;
    color: var(--text);
  }
  .cols {
    display: flex;
    gap: 18px;
    min-height: 0;
  }
  .cats {
    width: 168px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cats button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
  }
  .cats button:hover,
  .cats button.on {
    background: var(--nav-active);
  }
  .cats button.on {
    font-weight: 600;
  }
  .n {
    font-size: 12px;
    color: var(--muted);
    font-weight: 400;
  }
  .list-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .small {
    font-size: 12px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 14px 0 0;
    border-top: 1px solid var(--divider);
  }
  .list li:first-child {
    border-top: 0;
  }
  .list li.selected {
    background: var(--accent-soft);
  }
  .row-main {
    flex: 1;
    min-width: 0;
    text-align: left;
    border: 0;
    background: transparent;
    padding: 11px 0 11px 14px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font-weight: 500;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .chip {
    font-size: 11px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--chip);
    color: var(--muted);
  }
  .chip.safe {
    background: var(--green-soft);
    color: var(--green);
  }
  .chip.moderate {
    background: var(--amber-soft);
    color: var(--amber);
  }
  .chip.advanced {
    background: var(--red-soft);
    color: var(--red);
  }
  .chip.tag.accent {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .chip.tag.amber {
    background: var(--amber-soft);
    color: var(--amber);
  }
  .switch {
    width: 40px;
    height: 20px;
    flex: none;
    border-radius: 99px;
    padding: 0 3px;
    border: 1.5px solid var(--muted);
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: flex-start;
  }
  .switch span {
    width: 12px;
    height: 12px;
    border-radius: 99px;
    background: var(--muted);
  }
  .switch.on {
    border-color: var(--accent);
    background: var(--accent);
    justify-content: flex-end;
  }
  .switch.on span {
    background: #ffffff;
  }
  .switch:disabled {
    opacity: 0.45;
  }
  .detail {
    grid-row: 1;
    grid-column: 2;
    background: var(--surface);
    border-left: 1px solid var(--border);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow: auto;
  }
  .d-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }
  .d-head h2 {
    font-size: 16px;
    line-height: 1.3;
  }
  .icon-btn {
    width: 28px;
    height: 28px;
    flex: none;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .callout {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--chip);
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }
  .callout.amber {
    background: var(--amber-soft);
  }
  .admin {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  details {
    border-top: 1px solid var(--divider);
    padding-top: 10px;
  }
  .code {
    margin-top: 8px;
    font-family: Consolas, "Cascadia Mono", monospace;
    font-size: 11px;
    line-height: 1.6;
    background: var(--content);
    border-radius: 6px;
    padding: 8px 10px;
    overflow-wrap: anywhere;
    user-select: text;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .source {
    font-size: 12px;
    overflow-wrap: anywhere;
    user-select: text;
    color: var(--accent-text);
  }
  .bar {
    grid-column: 1 / -1;
    height: 60px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 20px 0 26px;
  }
  .bar strong {
    margin-right: 10px;
  }
  .bar-actions {
    display: flex;
    gap: 10px;
  }
  .note {
    padding: 14px 16px;
  }
  .error {
    color: var(--red);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  p {
    margin: 0;
  }
</style>
