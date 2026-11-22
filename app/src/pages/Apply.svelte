<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import Loader from "@lucide/svelte/icons/loader";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import X from "@lucide/svelte/icons/x";
  import { onMount } from "svelte";
  import BenchPanel from "../components/BenchPanel.svelte";
  import {
    benchStatus,
    restart,
    runHelper,
    type BlockReason,
    type HelperEvent,
    type HelperRequest,
    type Outcome,
    type Restart,
  } from "../lib/backend";
  import { helperError, naText } from "../lib/describe";
  import { prefs, t, ui } from "../lib/state.svelte";
  import { byId, clearPending, loadCatalog, pending } from "../lib/tweaks.svelte";

  type Line = { id: string; name: string; state: "waiting" | "working" | "ok" | "skipped" | "error"; note: string };
  type Phase = "bench" | "running" | "done" | "cancelled" | "failed";

  let phase = $state<Phase>("running");
  let lines = $state<Line[]>([]);
  let notes = $state<string[]>([]);
  let problem = $state<string | null>(null);
  let restartKind = $state<Restart>("none");

  const rank: Record<Restart, number> = { none: 0, explorer: 1, signout: 2, reboot: 3 };
  const done = $derived(lines.filter((l) => l.state !== "waiting" && l.state !== "working").length);
  const ok = $derived(lines.filter((l) => l.state === "ok").length);

  function outcomeLine(o: Outcome): Pick<Line, "state" | "note"> {
    switch (o.code) {
      case "applied":
        return { state: "ok", note: t("app.apply.applied") };
      case "already_applied":
        return { state: "ok", note: t("app.apply.already") };
      case "undone":
        return { state: "ok", note: t("app.apply.undone") };
      case "nothing_to_undo":
        return { state: "skipped", note: t("app.apply.nothing") };
      case "cleaned":
      case "repaired":
        return { state: "ok", note: t("app.apply.applied") };
      case "not_applicable":
        return { state: "skipped", note: t("app.apply.skipped", { reason: naText(o.reason) }) };
      case "failed":
        return {
          state: "error",
          note: t(o.rolled_back ? "app.apply.failed_clean" : "app.apply.failed_dirty", { error: o.error }),
        };
    }
  }

  function blockText(b: BlockReason): string {
    switch (b.code) {
      case "not_windows11":
        return t("app.blocked.not_windows11", { build: b.build });
      case "not_x64":
        return t("app.blocked.not_x64", { arch: b.arch });
      case "needs_admin":
        return t("app.blocked.needs_admin");
      case "game_running":
        return t("app.blocked.game_running", { list: b.processes.join(", ") });
      case "recovery_failed":
        return t("app.blocked.recovery_failed", { errors: b.errors.join("; ") });
      case "journal_untrusted":
        return t("app.blocked.journal_untrusted", { recipe: b.recipe });
    }
  }

  /** Updates the live list with one helper event. */
  function onEvent(e: HelperEvent): void {
    const line = (id: string) => lines.find((l) => l.id === id);
    switch (e.event) {
      case "started": {
        const l = line(e.id);
        if (l) {
          l.state = "working";
          l.note = t("app.apply.working");
        }
        break;
      }
      case "result": {
        const l = line(e.id);
        if (l) Object.assign(l, outcomeLine(e.outcome));
        break;
      }
      case "restore_point":
        notes.push(
          e.created
            ? t("app.apply.restore_point_ok")
            : t("app.apply.restore_point_skipped", { message: e.message ?? "" }),
        );
        break;
      case "recovered":
        notes.push(t("app.apply.recovered", { restored: e.restored }));
        break;
      case "done":
        if (rank[e.restart] > rank[restartKind]) restartKind = e.restart;
        break;
      case "blocked":
        problem = blockText(e.reason);
        break;
      case "refused":
        problem = t("app.refused", { code: e.reason.code });
        break;
      case "error":
        problem = t("app.helper_error", { message: e.message });
        break;
      case "hello":
        break;
    }
  }

  async function run() {
    const apply = [...pending.apply];
    const undo = [...pending.undo];
    if (apply.length + undo.length === 0) {
      ui.page = "tweaks";
      return;
    }
    phase = "running";
    problem = null;
    notes = [];
    restartKind = "none";
    lines = [...undo, ...apply].map((id) => ({
      id,
      name: byId(id)?.name ?? id,
      state: "waiting",
      note: t("app.apply.waiting"),
    }));
    // Undo first, then apply: each request is one UAC prompt.
    const requests: HelperRequest[] = [];
    if (undo.length) requests.push({ op: "undo", ids: undo });
    if (apply.length) requests.push({ op: "apply", ids: apply, restore_point: prefs.restorePoint });
    try {
      for (const request of requests) {
        await runHelper(request, onEvent);
        if (problem) break;
      }
      phase = problem ? "failed" : "done";
    } catch (e) {
      if (String(e).includes("cancelled")) phase = "cancelled";
      else {
        problem = helperError(e);
        phase = "failed";
      }
    }
    if (phase !== "cancelled") {
      // What succeeded is no longer pending; the rest stays ticked.
      const finished = new Set(lines.filter((l) => l.state !== "waiting" && l.state !== "working").map((l) => l.id));
      pending.apply = pending.apply.filter((id) => !finished.has(id));
      pending.undo = pending.undo.filter((id) => !finished.has(id));
      if (phase === "done") clearPending();
      void loadCatalog(ui.lang);
    }
  }

  // "Measure FPS first": the benchmark card comes before applying.
  let benchSaved = $state(false);
  let openBefore = $state<string | null>(null);

  function prepare() {
    lines = [...pending.undo, ...pending.apply].map((id) => ({
      id,
      name: byId(id)?.name ?? id,
      state: "waiting",
      note: t("app.apply.waiting"),
    }));
  }

  async function afterRun() {
    if (phase === "done" || phase === "failed") openBefore = (await benchStatus(null)).openBefore;
  }

  async function applyNow() {
    await run();
    await afterRun();
  }

  /** A real FPS drop: undo what was applied since the "before" measurement. */
  async function undoSince(ids: string[]) {
    clearPending();
    pending.undo = ids;
    openBefore = null;
    await applyNow();
  }

  onMount(() => {
    if (ui.benchFirst) {
      phase = "bench";
      prepare();
    } else void applyNow();
  });

  const restartButton: Record<Exclude<Restart, "none">, string> = {
    explorer: "app.apply.explorer_now",
    signout: "app.apply.signout_now",
    reboot: "app.apply.restart_now",
  };
</script>

<div class="page">
  <div>
    <button class="back" onclick={() => (ui.page = "tweaks")} disabled={phase === "running"}>
      <ChevronLeft size={14} aria-hidden="true" />{t("app.apply.back")}
    </button>
    {#if phase === "bench"}
      <h1>{t("app.bench.title_before")}</h1>
      <p class="muted">{t("app.bench.intro_before")}</p>
    {:else if phase === "running"}
      <h1>{t("app.apply.running", { count: lines.length })}</h1>
      <p class="muted">{t("app.apply.progress", { done, total: lines.length })} {t("app.apply.uac")}</p>
    {:else if phase === "done"}
      <h1>{t("app.apply.done_title", { ok, skipped: lines.length - ok })}</h1>
      <p class="muted">{t("app.apply.done_text")}</p>
    {:else if phase === "cancelled"}
      <h1>{t("app.apply.cancelled_title")}</h1>
      <p class="muted">{t("app.apply.cancelled_text")}</p>
    {:else}
      <h1>{t("app.apply.failed_title")}</h1>
      <p class="problem" role="alert">{problem}</p>
    {/if}
  </div>

  {#if (phase === "done" || phase === "failed") && restartKind !== "none"}
    {@const kind = restartKind}
    <div class="banner" role="status">
      <RotateCcw size={18} aria-hidden="true" />
      <div class="grow">{t(`app.apply.restart_${kind}`)}</div>
      <button class="btn" onclick={() => (restartKind = "none")}>{t("app.apply.later")}</button>
      <button class="btn primary" onclick={() => void restart(kind)}>{t(restartButton[kind])}</button>
    </div>
  {/if}

  {#each notes as n, i (i)}<p class="muted">{n}</p>{/each}

  <div class="grid">
    <ul class="card list" aria-live="polite">
      {#each lines as l (l.id)}
        <li>
          <span class="dot {l.state}" aria-hidden="true">
            {#if l.state === "ok"}<Check size={13} strokeWidth={3} />{:else if l.state === "error" || l.state === "skipped"}<X
                size={12}
                strokeWidth={3}
              />{:else if l.state === "working"}<Loader size={14} strokeWidth={2.6} />{/if}
          </span>
          <div class="grow">
            <div class="name">{l.name}</div>
            <div class="note {l.state}">{l.note}</div>
          </div>
        </li>
      {/each}
    </ul>
    <div class="side-col">
      {#if phase === "bench"}
        <BenchPanel label="before" onSaved={() => (benchSaved = true)} />
        <button class="btn" class:primary={benchSaved} onclick={() => void applyNow()}>
          {t(benchSaved ? "app.bench.apply_now" : "app.bench.skip")}
        </button>
      {:else if phase === "running"}
        <aside class="card side"><p class="muted">{t("app.apply.keep_open")}</p></aside>
      {:else if phase === "cancelled"}
        <aside class="card side">
          <button class="btn primary" onclick={() => void applyNow()}>{t("app.apply.retry")}</button>
        </aside>
      {:else if openBefore}
        <BenchPanel label="after" target={openBefore} onUndo={(ids) => void undoSince(ids)} />
      {:else}
        <aside class="card side"><p class="muted">{t("app.apply.done_text")}</p></aside>
      {/if}
    </div>
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: transparent;
    padding: 0;
    color: var(--accent-text);
    font-weight: 500;
    font-size: 12px;
  }
  .back:disabled {
    opacity: 0.5;
  }
  h1 {
    margin-top: 4px;
  }
  p {
    margin: 2px 0 0;
  }
  .problem {
    color: var(--red);
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--text);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 16px;
    align-items: start;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }
  .list li {
    display: flex;
    gap: 12px;
    padding: 13px 16px;
    border-top: 1px solid var(--divider);
  }
  .list li:first-child {
    border-top: 0;
  }
  .dot {
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 99px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--chip);
    color: var(--muted);
  }
  .dot.ok {
    background: var(--green-soft);
    color: var(--green);
  }
  .dot.error {
    background: var(--red-soft);
    color: var(--red);
  }
  .dot.working {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .name {
    font-weight: 500;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
  }
  .note.error {
    color: var(--red);
  }
  .note.working {
    color: var(--accent-text);
  }
  .side-col {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .side {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
</style>
