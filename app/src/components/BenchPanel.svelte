<script lang="ts">
  import Gauge from "@lucide/svelte/icons/gauge";
  import {
    benchGames,
    benchStatus,
    cancelHelper,
    launchGame,
    runHelper,
    type BenchGame,
    type HelperEvent,
    type Metrics,
    type Verdict,
  } from "../lib/backend";
  import { helperError } from "../lib/describe";
  import { t, ui } from "../lib/state.svelte";

  interface Props {
    label: "before" | "after";
    /** The target measured before (for "after"). */
    target?: string | null;
    /** Called when a series is saved (with how many runs). */
    onSaved?: (runs: number) => void;
    /** "Undo the changes" after a real FPS drop. */
    onUndo?: (ids: string[]) => void;
  }
  let { label, target = null, onSaved, onUndo }: Props = $props();

  const RUNS = 3;
  const DURATION = 60;
  const WARMUP = 10;

  let games = $state<BenchGame[]>([]);
  let choice = $state<string>("");
  let exe = $state("");
  let phase = $state<"pick" | "running" | "finished">("pick");
  let status = $state("");
  let error = $state<string | null>(null);
  let results = $state<Metrics[]>([]);
  let saved = $state(0);
  let verdict = $state<Verdict | null>(null);

  $effect(() => {
    void benchGames().then((list) => {
      games = list;
      if (!choice) choice = target ?? list.find((g) => g.installed)?.key ?? "other";
    });
  });

  const game = $derived(games.find((g) => g.key === choice) ?? null);
  const exeValid = $derived(/^[A-Za-z0-9 ._\-()+]{1,76}\.exe$/i.test(exe.trim()));
  const targetKey = $derived(choice === "other" ? exe.trim() : choice);
  const targetName = (key: string | null) => games.find((g) => g.key === key)?.name ?? key ?? "";
  const canStart = $derived(choice === "other" ? exeValid : !!game?.installed);

  function fmt(n: number): string {
    const s = n.toFixed(1);
    return ui.lang === "ro" ? s.replace(".", ",") : s;
  }
  const pct = (n: number) => `${n >= 0 ? "+" : ""}${fmt(n)}%`;

  function onEvent(e: HelperEvent) {
    switch (e.event) {
      case "bench_waiting_game":
        status = t("app.bench.waiting_game", { processes: e.processes.join(" / ") });
        break;
      case "bench_waiting_key":
        status = t("app.bench.press", { run: e.run, runs: e.runs });
        break;
      case "bench_measuring":
        status = t("app.bench.measuring", { seconds: e.seconds });
        break;
      case "bench_run":
        results.push(e.metrics);
        break;
      case "bench_finished":
        saved = e.runs;
        break;
      case "error":
        error = t("app.helper_error", { message: e.message });
        break;
      case "blocked":
        error = e.reason.code === "needs_admin" ? t("app.blocked.needs_admin") : t("app.helper_error", { message: e.reason.code });
        break;
      case "refused":
        error = t("app.refused", { code: e.reason.code });
        break;
      default:
        break;
    }
  }

  async function start() {
    const key = targetKey;
    phase = "running";
    error = null;
    results = [];
    saved = 0;
    verdict = null;
    status = "";
    if (game?.canLaunch && choice !== "other") {
      status = t("app.bench.launching", { game: game.name });
      try {
        await launchGame(key);
      } catch (e) {
        status = t("app.bench.launch_failed", { game: game.name, details: String(e) });
      }
    }
    try {
      await runHelper(
        { op: "bench", target: key, label, runs: RUNS, duration: DURATION, warmup: WARMUP },
        onEvent,
      );
    } catch (e) {
      error = String(e).includes("cancelled") ? t("app.apply.cancelled_text") : helperError(e);
    }
    phase = "finished";
    if (saved > 0) {
      onSaved?.(saved);
      if (label === "after") verdict = (await benchStatus(key)).verdict;
    }
  }
</script>

<section class="card bench" aria-labelledby="bench-title">
  <h2 id="bench-title"><Gauge size={16} aria-hidden="true" /> {t(label === "before" ? "app.bench.title_before" : "app.bench.title_after")}</h2>

  {#if phase === "pick"}
    <p class="muted">
      {label === "after" && target ? t("app.bench.intro_after", { game: targetName(target) }) : t("app.bench.intro_before")}
    </p>
    {#if !(label === "after" && target)}
      <label class="field">
        <span class="muted small">{t("app.bench.game")}</span>
        <select bind:value={choice}>
          {#each games as g (g.key)}
            <option value={g.key} disabled={!g.installed}>{g.installed ? g.name : t("app.bench.not_installed", { name: g.name })}</option>
          {/each}
          <option value="other">{t("app.bench.other")}</option>
        </select>
      </label>
      {#if choice === "other"}
        <label class="field">
          <span class="muted small">{t("app.bench.exe")}</span>
          <input type="text" bind:value={exe} placeholder={t("app.bench.exe_hint")} spellcheck="false" />
          {#if exe && !exeValid}<span class="bad small">{t("app.bench.exe_bad")}</span>{/if}
        </label>
      {/if}
    {/if}
    {#if game && !game.precise}<p class="muted small">{t("app.bench.less_precise", { game: game.name })}</p>{/if}
    <button class="btn primary" disabled={!canStart} onclick={start}>{t("app.bench.start")}</button>
    <p class="muted small">{t("app.bench.uac")}</p>
  {:else}
    {#if phase === "running" && status}<p class="status" aria-live="polite">{status}</p>{/if}
    {#if results.length}
      <ul class="runs">
        {#each results as m, i (i)}
          <li>{t("app.bench.run_result", { run: i + 1, fps: fmt(m.avg_fps), low: fmt(m.low1_fps) })}</li>
        {/each}
      </ul>
    {/if}
    {#if error}<p class="bad" role="alert">{error}</p>{/if}
    {#if phase === "running"}
      <button class="btn" onclick={() => void cancelHelper()}>{t("app.bench.stop")}</button>
    {:else}
      <p class="muted">{saved > 0 ? t("app.bench.saved", { runs: saved }) : t("app.bench.nothing_saved")}</p>
      {#if verdict}
        {@const c = verdict.comparison}
        <h3>{t("app.bench.verdict_title", { game: targetName(verdict.target) })}</h3>
        <dl class="metrics">
          <dt>{t("app.bench.fps")}</dt>
          <dd>{fmt(c.before.avg_fps)} → {fmt(c.after.avg_fps)} <span class:up={c.fps_change_pct >= 0} class:down={c.fps_change_pct < 0}>{pct(c.fps_change_pct)}</span></dd>
          <dt>{t("app.bench.low")}</dt>
          <dd>{fmt(c.before.low1_fps)} → {fmt(c.after.low1_fps)} <span class:up={c.low1_change_pct >= 0} class:down={c.low1_change_pct < 0}>{pct(c.low1_change_pct)}</span></dd>
          <dt>{t("app.bench.ft")}</dt>
          <dd>{fmt(c.before.avg_frametime_ms)} → {fmt(c.after.avg_frametime_ms)} ms</dd>
        </dl>
        {#if verdict.kind === "better"}
          <div class="verdict good">{t("app.bench.better", { change: pct(c.fps_change_pct) })}</div>
        {:else if verdict.kind === "same"}
          <div class="verdict">{t("app.bench.same")}</div>
        {:else if verdict.undoIds.length}
          {@const ids = verdict.undoIds}
          <div class="verdict bad-box">{t("app.bench.drop")}</div>
          <button class="btn primary" onclick={() => onUndo?.(ids)}>{t("app.bench.undo")}</button>
        {:else}
          <div class="verdict bad-box">{t("app.bench.drop_unknown")}</div>
        {/if}
        <p class="muted small">{t("app.bench.verdict_note")}</p>
      {/if}
      <button class="btn" onclick={() => (phase = "pick")}>{t("app.bench.again")}</button>
    {/if}
  {/if}
</section>

<style>
  .bench {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: stretch;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    margin: 4px 0 0;
    font-size: 13px;
    font-weight: 600;
  }
  p {
    margin: 0;
  }
  .small {
    font-size: 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  select,
  input {
    height: 32px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    padding: 0 8px;
  }
  .btn {
    justify-content: center;
  }
  .status {
    font-weight: 600;
    color: var(--accent-text);
  }
  .runs {
    margin: 0;
    padding-left: 18px;
  }
  .bad {
    color: var(--red);
  }
  .metrics {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 12px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    text-align: right;
    font-weight: 600;
  }
  .up {
    color: var(--green);
  }
  .down {
    color: var(--red);
  }
  .verdict {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--chip);
    font-weight: 600;
  }
  .verdict.good {
    background: var(--green-soft);
    color: var(--green);
  }
  .verdict.bad-box {
    background: var(--red-soft);
    color: var(--red);
  }
</style>
