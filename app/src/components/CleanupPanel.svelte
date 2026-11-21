<script lang="ts">
  // The "Cleanup" category: tick what to delete, see how much it frees, confirm, clean.
  // Nothing here can be undone, so it always asks first and nothing is ticked by default.
  import AlertTriangle from "@lucide/svelte/icons/alert-triangle";
  import Loader from "@lucide/svelte/icons/loader";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { onMount } from "svelte";
  import { cleanupSizes, cleanUser, runHelper, type CleanSize, type Recipe } from "../lib/backend";
  import { formatBytes, helperError } from "../lib/describe";
  import { t, ui } from "../lib/state.svelte";

  let { recipes, onSelect }: { recipes: Recipe[]; onSelect: (id: string) => void } = $props();

  type Phase = "pick" | "confirm" | "running" | "done";
  let phase = $state<Phase>("pick");
  let sizes = $state<Record<string, CleanSize>>({});
  let picked = $state<string[]>([]);
  let freed = $state<Record<string, number>>({});
  let errors = $state<string[]>([]);

  async function measure() {
    try {
      sizes = Object.fromEntries((await cleanupSizes()).map((s) => [s.id, s]));
    } catch (e) {
      errors = [String(e)];
    }
  }
  onMount(measure);

  const size = (id: string) => sizes[id]?.bytes;
  const known = $derived(picked.reduce((sum, id) => sum + (size(id) ?? 0), 0));
  const someUnknown = $derived(picked.some((id) => size(id) == null));
  const needsAdmin = $derived(picked.some((id) => sizes[id]?.needsAdmin ?? recipes.find((r) => r.id === id)?.needsAdmin));
  const total = $derived(Object.values(freed).reduce((a, b) => a + b, 0));

  function flip(id: string) {
    picked = picked.includes(id) ? picked.filter((x) => x !== id) : [...picked, id];
  }

  async function clean() {
    phase = "running";
    errors = [];
    freed = {};
    const ids = [...picked];
    try {
      for (const r of await cleanUser(ids)) {
        freed[r.id] = (freed[r.id] ?? 0) + r.freedBytes;
        if (r.error) errors.push(r.error);
      }
      const system = ids.filter((id) => sizes[id]?.needsAdmin);
      if (system.length) {
        await runHelper({ op: "clean", ids: system }, (e) => {
          if (e.event === "result" && e.outcome.code === "cleaned") {
            freed[e.id] = (freed[e.id] ?? 0) + e.outcome.freed_bytes;
          } else if (e.event === "result" && e.outcome.code === "failed") {
            errors.push(e.outcome.error);
          } else if (e.event === "blocked" || e.event === "refused") {
            errors.push(t("app.clean.not_done"));
          } else if (e.event === "error") {
            errors.push(e.message);
          }
        });
      }
    } catch (e) {
      // "cancelled": No at the UAC prompt; the user's own files were still cleaned.
      errors.push(String(e) === "cancelled" ? t("app.clean.uac_no") : helperError(e));
    }
    phase = "done";
    picked = [];
    await measure();
  }
</script>

<div class="cleanup">
  <div class="callout amber" role="note">
    <AlertTriangle size={15} aria-hidden="true" />
    <span>{t("app.clean.warning")}</span>
  </div>

  <ul class="card list">
    {#each recipes as r (r.id)}
      <li>
        <label class="row">
          <input type="checkbox" checked={picked.includes(r.id)} disabled={phase === "running"} onchange={() => flip(r.id)} />
          <span class="text">
            <span class="name">{r.name}</span>
            <span class="muted small">
              {#if size(r.id) != null}
                {formatBytes(size(r.id) ?? 0, ui.lang)}
              {:else if sizes[r.id]}
                {t("app.clean.size_unknown")}
              {:else}
                {t("app.clean.measuring")}
              {/if}
              {#if freed[r.id] != null}
                · {t("app.clean.freed_one", { size: formatBytes(freed[r.id] ?? 0, ui.lang) })}
              {/if}
            </span>
          </span>
          {#if r.needsAdmin}<span title={t("app.detail.admin")}><ShieldCheck size={14} aria-hidden="true" /></span>{/if}
        </label>
        <button class="btn link" onclick={() => onSelect(r.id)}>{t("app.clean.details")}</button>
      </li>
    {/each}
  </ul>

  {#if phase === "confirm"}
    <div class="card confirm" role="alertdialog" aria-labelledby="clean-q">
      <strong id="clean-q">
        {t("app.clean.confirm", { count: picked.length, size: formatBytes(known, ui.lang) })}
      </strong>
      {#if someUnknown}<span class="muted small">{t("app.clean.confirm_unknown")}</span>{/if}
      <span class="muted small">{t("app.clean.confirm_note")}</span>
      {#if needsAdmin}<span class="muted small">{t("app.clean.confirm_uac")}</span>{/if}
      <div class="actions">
        <button class="btn" onclick={() => (phase = "pick")}>{t("app.clean.cancel")}</button>
        <button class="btn primary danger" onclick={clean}><Trash2 size={15} aria-hidden="true" />{t("app.clean.do")}</button>
      </div>
    </div>
  {:else}
    <div class="actions">
      {#if phase === "running"}
        <span class="muted"><Loader size={15} aria-hidden="true" /> {t("app.clean.running")}</span>
      {:else if phase === "done"}
        <span role="status">{t("app.clean.done", { size: formatBytes(total, ui.lang) })}</span>
      {/if}
      <button class="btn primary" disabled={picked.length === 0 || phase === "running"} onclick={() => (phase = "confirm")}>
        <Trash2 size={15} aria-hidden="true" />{t("app.clean.button")}
      </button>
    </div>
  {/if}

  {#each errors as e, i (i)}
    <div class="callout" role="alert">{e}</div>
  {/each}
</div>

<style>
  .cleanup {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .callout {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--chip);
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .callout.amber {
    background: var(--amber-soft);
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
    gap: 8px;
    padding-right: 10px;
    border-top: 1px solid var(--divider);
  }
  .list li:first-child {
    border-top: 0;
  }
  .row {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 14px;
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font-weight: 500;
  }
  .small {
    font-size: 12px;
  }
  .btn.link {
    border: 0;
    background: transparent;
    color: var(--accent);
    height: 28px;
    padding: 0 6px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
  }
  .confirm {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .btn.danger {
    background: var(--red, #c42b1c);
  }
  .btn:disabled {
    opacity: 0.5;
  }
</style>
