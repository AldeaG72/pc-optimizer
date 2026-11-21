<script lang="ts">
  import AlertTriangle from "@lucide/svelte/icons/alert-triangle";
  import AppWindow from "@lucide/svelte/icons/app-window";
  import BatteryMedium from "@lucide/svelte/icons/battery-medium";
  import Cpu from "@lucide/svelte/icons/cpu";
  import Gamepad2 from "@lucide/svelte/icons/gamepad-2";
  import HardDrive from "@lucide/svelte/icons/hard-drive";
  import Info from "@lucide/svelte/icons/info";
  import MemoryStick from "@lucide/svelte/icons/memory-stick";
  import Monitor from "@lucide/svelte/icons/monitor";
  import OctagonAlert from "@lucide/svelte/icons/alert-octagon";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import { home, recipes, type HomeView, type HwRow, type Recipe } from "../lib/backend";
  import { openRecipe, persist, prefs, t, ui } from "../lib/state.svelte";

  const icons: Record<HwRow["kind"], typeof Cpu> = {
    cpu: Cpu,
    gpu: Monitor,
    memory: MemoryStick,
    display: Monitor,
    disk: HardDrive,
    windows: AppWindow,
    power: BatteryMedium,
    games: Gamepad2,
  };
  const DISPLAY_CHECKS = new Set(["display_refresh", "display_resolution", "hdr"]);

  let view = $state<HomeView | null>(null);
  let list = $state<Recipe[]>([]);
  let error = $state<string | null>(null);
  let open = $state<Record<number, boolean>>({});

  async function load() {
    view = null;
    error = null;
    try {
      const lang = ui.lang;
      const [h, catalog] = await Promise.all([home(lang), recipes(lang)]);
      view = h;
      list = catalog.recipes;
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void ui.lang;
    void load();
  });

  // With several monitors, only the one the user plays on matters.
  const monitor = $derived(
    prefs.gameMonitor ?? view?.displays.find((d) => d.primary)?.name ?? null,
  );
  const findings = $derived(
    (view?.findings ?? []).filter(
      (f) =>
        !(view && view.displays.length > 1 && DISPLAY_CHECKS.has(f.check) && f.subject !== monitor),
    ),
  );

  const counts = $derived.by(() => {
    const profile = view?.laptop ? "laptop" : "recommended";
    return {
      applied: list.filter((r) => r.status.code === "applied").length,
      outside: list.filter((r) => r.status.code === "modified_outside").length,
      recommended: list.filter(
        (r) => r.risk === "safe" && r.profiles.includes(profile) && r.status.code === "not_applied",
      ).length,
    };
  });

  function chooseMonitor(name: string) {
    prefs.gameMonitor = name;
    void persist();
  }

  function seeRecommended() {
    ui.focus = "recommended";
    ui.page = "tweaks";
  }
</script>

<div class="page">
  <div class="head">
    <div>
      <h1>{t("app.home.title")}</h1>
      <p class="muted">{view ? t("app.home.subtitle") : error ? "" : t("app.home.checking")}</p>
    </div>
    <button class="btn" onclick={load} disabled={!view && !error}>
      <RotateCcw size={15} aria-hidden="true" />
      {t("app.home.recheck")}
    </button>
  </div>

  {#if error}
    <div class="card error" role="alert">{t("app.home.error", { message: error })}</div>
  {/if}

  {#if view}
    <div class="grid">
      <section class="card pc" aria-labelledby="pc">
        <h2 id="pc">{t("app.home.pc")}</h2>
        {#each view.hardware as row, i (i)}
          {@const Icon = icons[row.kind]}
          <div class="hw">
            <span class="hw-icon" aria-hidden="true"><Icon size={16} strokeWidth={1.8} /></span>
            <div>
              <div class="muted small">{row.label}</div>
              <div class="value">{row.value}</div>
              {#if row.detail}<div class="muted small">{row.detail}</div>{/if}
            </div>
          </div>
        {/each}
        {#if view.displays.length > 1}
          <label class="monitor">
            <span class="muted small">{t("app.home.monitor")}</span>
            <select value={monitor} onchange={(e) => chooseMonitor(e.currentTarget.value)}>
              {#each view.displays as d (d.name)}
                <option value={d.name}>{d.primary ? t("app.home.monitor_primary", { name: d.name }) : d.name}</option>
              {/each}
            </select>
          </label>
        {/if}
      </section>

      <div class="right">
        <section aria-labelledby="found" class="found">
          <div class="found-head">
            <h2 id="found">{t("app.home.found")}</h2>
            <span class="count">{findings.length}</span>
          </div>
          {#if findings.length === 0}
            <div class="card note muted">{t("app.home.nothing")}</div>
          {/if}
          {#each findings as f, i (i)}
            <article class="card finding">
              <span class="badge {f.outcome}" aria-hidden="true">
                {#if f.outcome === "problem"}<OctagonAlert size={17} />{:else if f.outcome === "warning"}<AlertTriangle
                    size={17}
                  />{:else}<Info size={17} />{/if}
              </span>
              <div class="text">
                <div class="title">
                  <span class="sr">{t(`app.outcome.${f.outcome}`)}: </span>{f.title}{#if f.subject}<span class="muted"> · {f.subject}</span>{/if}
                </div>
                <div class="muted">{f.message}</div>
                {#if open[i]}
                  <div class="fix"><strong>{t("app.home.how")}</strong> {f.fix}</div>
                  {#if f.link}<div class="link muted small">{t("app.home.link")}: {f.link}</div>{/if}
                {/if}
              </div>
              <div class="actions">
                <button class="btn small-btn" aria-expanded={!!open[i]} onclick={() => (open[i] = !open[i])}
                  >{t(open[i] ? "app.home.hide" : "app.home.details")}</button
                >
                {#if f.recipe}
                  {@const id = f.recipe}
                  <button class="btn primary small-btn" onclick={() => openRecipe(id)}>{t("app.home.fix")}</button>
                {/if}
              </div>
            </article>
          {/each}
          {#if view.ok.length}
            <p class="muted small">{t("app.home.ok", { count: view.ok.length, list: view.ok.join(", ") })}</p>
          {/if}
          {#if view.unknown.length}
            <p class="muted small">{t("app.home.unknown", { list: view.unknown.join(", ") })}</p>
          {/if}
        </section>

        <section class="card tweaks" aria-labelledby="tw">
          <div class="tw-text">
            <h2 id="tw">{t("app.home.tweaks")}</h2>
            <div class="stats">
              <span>{t("app.home.applied", { count: counts.applied })}</span>
              {#if counts.outside > 0}<span class="amber">{t("app.home.outside", { count: counts.outside })}</span>{/if}
              <span>{t("app.home.recommended", { count: counts.recommended })}</span>
            </div>
          </div>
          <button class="btn primary" onclick={seeRecommended}>{t("app.home.see_recommended")}</button>
        </section>
      </div>
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
  }
  .head p {
    margin: 2px 0 0;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  .pc {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hw {
    display: flex;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--divider);
  }
  .hw-icon {
    width: 18px;
    flex: none;
    color: var(--muted);
    padding-top: 1px;
  }
  .value {
    font-weight: 500;
  }
  .small {
    font-size: 12px;
  }
  .monitor {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--divider);
  }
  select {
    height: 32px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    padding: 0 8px;
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  .found {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .found-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .count {
    font-size: 12px;
    padding: 0 8px;
    border-radius: 99px;
    background: var(--chip);
    color: var(--muted);
    font-weight: 500;
  }
  .note {
    padding: 14px 16px;
  }
  .finding {
    padding: 14px 16px;
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .badge {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--chip);
    color: var(--muted);
  }
  .badge.problem {
    background: var(--red-soft);
    color: var(--red);
  }
  .badge.warning {
    background: var(--amber-soft);
    color: var(--amber);
  }
  .badge.info {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .fix {
    margin-top: 6px;
  }
  .link {
    overflow-wrap: anywhere;
    user-select: text;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex: none;
  }
  .small-btn {
    height: 32px;
    padding: 0 12px;
  }
  .tweaks {
    padding: 16px 18px;
    display: flex;
    align-items: center;
    gap: 24px;
  }
  .tw-text {
    flex: 1;
    min-width: 0;
  }
  .stats {
    display: flex;
    gap: 6px 22px;
    flex-wrap: wrap;
    margin-top: 6px;
  }
  .amber {
    color: var(--amber);
  }
  .error {
    padding: 14px 16px;
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
