<script lang="ts">
  // Programs that start with Windows (3.12): what each is, an estimated impact, advice,
  // and a switch. Changes are collected and applied together (one UAC prompt at most);
  // each one goes through the journal, so "Undo all" turns them back on.
  import Loader from "@lucide/svelte/icons/loader";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import { onMount } from "svelte";
  import { runHelper, startupList, type StartupView } from "../lib/backend";
  import { formatBytes, helperError } from "../lib/describe";
  import { t, ui } from "../lib/state.svelte";

  let list = $state<StartupView[] | null>(null);
  let error = $state<string | null>(null);
  /** id → wanted state, only for entries the user flipped. */
  let wanted = $state<Record<string, boolean>>({});
  let running = $state(false);
  let message = $state<string | null>(null);

  async function load() {
    try {
      list = await startupList();
      error = null;
    } catch (e) {
      error = String(e);
    }
  }
  onMount(load);

  const on = (s: StartupView) => wanted[s.id] ?? s.enabled;
  const changes = $derived(
    (list ?? []).filter((s) => wanted[s.id] !== undefined && wanted[s.id] !== s.enabled),
  );

  function flip(s: StartupView) {
    const next = !on(s);
    if (next === s.enabled) {
      const { [s.id]: _, ...rest } = wanted;
      wanted = rest;
    } else {
      wanted = { ...wanted, [s.id]: next };
    }
  }

  async function apply() {
    running = true;
    message = null;
    let ok = 0;
    const problems: string[] = [];
    try {
      await runHelper({ op: "startup", changes: changes.map((s) => ({ id: s.id, enable: !s.enabled })) }, (e) => {
        if (e.event === "result") {
          if (e.outcome.code === "failed") problems.push(e.outcome.error);
          else ok += 1;
        } else if (e.event === "blocked") {
          problems.push(t("app.startup.blocked"));
        } else if (e.event === "refused" || e.event === "error") {
          problems.push(t("app.startup.refused"));
        }
      });
    } catch (e) {
      problems.push(String(e) === "cancelled" ? t("app.startup.uac_no") : helperError(e));
    }
    message = problems.length ? problems.join(" ") : t("app.startup.done", { count: ok });
    wanted = {};
    running = false;
    await load();
  }
</script>

<div class="startup">
  <p class="muted small">{t("app.startup.intro")}</p>

  {#if error}
    <div class="callout" role="alert">{t("app.startup.error", { message: error })}</div>
  {:else if list === null}
    <p class="muted">{t("app.startup.loading")}</p>
  {:else if list.length === 0}
    <div class="card note muted">{t("app.startup.none")}</div>
  {:else}
    <ul class="card list">
      {#each list as s (s.id)}
        <li>
          <div class="text">
            <span class="name">{s.name}</span>
            <span class="muted small">{s.what ? t(`app.startup.what.${s.what}`) : s.command}</span>
            <span class="meta small">
              <span class="chip {s.impact}">{t(`app.startup.impact.${s.impact}`)}</span>
              {#if s.size != null}<span class="muted">{formatBytes(s.size, ui.lang)}</span>{/if}
              {#if s.advice}<span class="advice {s.advice}">{t(`app.startup.advice.${s.advice}`)}</span>{/if}
              {#if s.needsAdmin}<span class="muted" title={t("app.startup.admin")}><ShieldCheck size={12} aria-hidden="true" /></span>{/if}
            </span>
          </div>
          <button
            class="switch"
            class:on={on(s)}
            role="switch"
            aria-checked={on(s)}
            aria-label={s.name}
            disabled={running}
            onclick={() => flip(s)}
          ><span></span></button>
        </li>
      {/each}
    </ul>
  {/if}

  <p class="muted small">{t("app.startup.store")}</p>

  <div class="actions">
    {#if running}<span class="muted"><Loader size={15} aria-hidden="true" /> {t("app.startup.running")}</span>{/if}
    {#if message}<span role="status">{message}</span>{/if}
    <button class="btn primary" disabled={changes.length === 0 || running} onclick={apply}>
      {t("app.startup.apply", { count: changes.length })}
    </button>
  </div>
</div>

<style>
  .startup {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .small {
    font-size: 12px;
  }
  .callout {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--chip);
  }
  .note {
    padding: 14px;
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
    padding: 10px 14px;
    border-top: 1px solid var(--divider);
  }
  .list li:first-child {
    border-top: 0;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .text > .muted {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-weight: 500;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  .meta > * {
    white-space: nowrap;
  }
  .chip {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--chip);
  }
  .chip.high {
    background: var(--amber-soft);
  }
  .advice.off {
    color: var(--accent);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .switch {
    flex: none;
    width: 40px;
    height: 20px;
    border-radius: 999px;
    border: 1px solid var(--border-strong, var(--border));
    background: var(--surface);
    position: relative;
    padding: 0;
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--muted);
    transition: left 0.12s;
  }
  .switch.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch.on span {
    left: 23px;
    background: var(--on-accent);
  }
  .switch:disabled {
    opacity: 0.5;
  }
</style>
