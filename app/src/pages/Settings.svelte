<script lang="ts">
  import { appVersion, benchForget, benchHistory, type Series } from "../lib/backend";
  import type { Lang } from "../lib/i18n";
  import { persist, prefs, t, ui, type Theme } from "../lib/state.svelte";
  import { catalog, clearPending, isApplied, loadCatalog, pending } from "../lib/tweaks.svelte";

  const themes: { value: Theme; key: string }[] = [
    { value: "system", key: "app.settings.theme_system" },
    { value: "light", key: "app.settings.theme_light" },
    { value: "dark", key: "app.settings.theme_dark" },
  ];

  let history = $state<Series[]>([]);
  let version = $state("");
  let confirm = $state<"advanced" | "history" | "undo" | null>(null);

  $effect(() => {
    void benchHistory().then((h) => (history = h));
    void appVersion().then((v) => (version = v));
    void loadCatalog(ui.lang);
  });

  const applied = $derived(catalog.list.filter(isApplied));
  const measurements = $derived(history.reduce((n, s) => n + s.runs.length, 0));

  function fmt(n: number): string {
    const s = n.toFixed(1);
    return ui.lang === "ro" ? s.replace(".", ",") : s;
  }
  const date = (ms: number) =>
    new Date(ms).toLocaleDateString(ui.lang === "ro" ? "ro-RO" : "en-GB", { day: "numeric", month: "short" });
  const avg = (s: Series) => s.runs.reduce((a, m) => a + m.avg_fps, 0) / Math.max(1, s.runs.length);

  function toggleRestorePoint() {
    prefs.restorePoint = !prefs.restorePoint;
    void persist();
  }

  function toggleAdvanced() {
    if (prefs.showAdvanced) {
      prefs.showAdvanced = false;
      void persist();
    } else confirm = "advanced";
  }

  function showAdvanced() {
    prefs.showAdvanced = true;
    confirm = null;
    void persist();
  }

  async function forget() {
    await benchForget();
    history = await benchHistory();
    confirm = null;
  }

  function undoAll() {
    clearPending();
    pending.undo = applied.map((r) => r.id);
    confirm = null;
    ui.benchFirst = false;
    ui.page = "apply";
  }
</script>

<div class="page">
  <h1>{t("app.settings.title")}</h1>

  <div class="cols">
    <div class="col">
      <section class="card" aria-labelledby="general">
        <h2 id="general">{t("app.settings.general")}</h2>
        <div class="row">
          <label for="lang">{t("app.settings.language")}</label>
          <select id="lang" bind:value={ui.lang}>
            <option value={"ro" satisfies Lang}>Română</option>
            <option value={"en" satisfies Lang}>English</option>
          </select>
        </div>
        <div class="row">
          <span id="theme">{t("app.settings.theme")}</span>
          <div class="segmented" role="radiogroup" aria-labelledby="theme">
            {#each themes as th (th.value)}
              <button role="radio" aria-checked={ui.theme === th.value} class:on={ui.theme === th.value} onclick={() => (ui.theme = th.value)}
                >{t(th.key)}</button
              >
            {/each}
          </div>
        </div>
      </section>

      <section class="card" aria-labelledby="safety">
        <h2 id="safety">{t("app.settings.safety")}</h2>
        <div class="row top">
          <div>
            <div class="strong">{t("app.settings.restore_point")}</div>
            <div class="muted small">{t("app.settings.restore_point_hint")}</div>
          </div>
          <button class="switch" class:on={prefs.restorePoint} role="switch" aria-checked={prefs.restorePoint} aria-label={t("app.settings.restore_point")} onclick={toggleRestorePoint}><span></span></button>
        </div>
        <div class="row top">
          <div>
            <div class="strong">{t("app.settings.advanced")}</div>
            <div class="muted small">{t("app.settings.advanced_hint")}</div>
          </div>
          <button class="switch" class:on={prefs.showAdvanced} role="switch" aria-checked={prefs.showAdvanced} aria-label={t("app.settings.advanced")} onclick={toggleAdvanced}><span></span></button>
        </div>
        {#if confirm === "advanced"}
          <div class="confirm" role="alertdialog" aria-label={t("app.settings.advanced")}>
            <p>{t("app.settings.advanced_confirm")}</p>
            <div class="buttons">
              <button class="btn" onclick={() => (confirm = null)}>{t("app.settings.cancel")}</button>
              <button class="btn primary" onclick={showAdvanced}>{t("app.settings.advanced_yes")}</button>
            </div>
          </div>
        {/if}
      </section>

      <section class="card" aria-labelledby="undo">
        <div class="row top">
          <div>
            <h2 id="undo">{t("app.settings.undo_all")}</h2>
            <div class="muted small">{applied.length ? t("app.settings.undo_all_hint") : t("app.settings.undo_all_none")}</div>
          </div>
          <button class="btn danger" disabled={applied.length === 0} onclick={() => (confirm = "undo")}>{t("app.settings.undo_all")}</button>
        </div>
        {#if confirm === "undo"}
          <div class="confirm" role="alertdialog" aria-label={t("app.settings.undo_all")}>
            <p>{t("app.settings.undo_all_confirm", { count: applied.length })}</p>
            <div class="buttons">
              <button class="btn" onclick={() => (confirm = null)}>{t("app.settings.cancel")}</button>
              <button class="btn primary" onclick={undoAll}>{t("app.settings.undo_all_yes")}</button>
            </div>
          </div>
        {/if}
      </section>
    </div>

    <div class="col">
      <section class="card" aria-labelledby="hist">
        <div class="row">
          <h2 id="hist">{t("app.settings.history")}</h2>
          <button class="btn small-btn" disabled={history.length === 0} onclick={() => (confirm = "history")}>{t("app.settings.history_forget")}</button>
        </div>
        {#if confirm === "history"}
          <div class="confirm" role="alertdialog" aria-label={t("app.settings.history_forget")}>
            <p>{t("app.settings.history_confirm", { count: measurements })}</p>
            <div class="buttons">
              <button class="btn" onclick={() => (confirm = null)}>{t("app.settings.cancel")}</button>
              <button class="btn primary" onclick={() => void forget()}>{t("app.settings.history_yes")}</button>
            </div>
          </div>
        {/if}
        {#if history.length === 0}
          <p class="muted">{t("app.settings.history_empty")}</p>
        {:else}
          <ul class="history">
            {#each history as s (s.id + s.label)}
              <li>
                <span class="muted">{date(s.id)}</span>
                <span class="strong">{s.target}</span>
                <span class="muted">{t(s.label === "before" ? "app.settings.label_before" : "app.settings.label_after")}</span>
                <span>{t("app.settings.history_line", { runs: s.runs.length, fps: fmt(avg(s)) })}</span>
              </li>
            {/each}
          </ul>
        {/if}
        <p class="muted small">{t("app.settings.history_note")}</p>
      </section>

      <section class="card" aria-labelledby="about">
        <h2 id="about">{t("app.settings.about")}</h2>
        <p>{t("app.settings.version", { version })}</p>
        <p class="muted">{t("app.settings.about_text")}</p>
        <p class="muted small">{t("app.disclaimer")}</p>
        <p class="muted small">{t("app.settings.not_done")}</p>
        <p class="muted small">{t("app.settings.licenses")}</p>
      </section>
    </div>
  </div>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    align-items: start;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  section {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  p {
    margin: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .row.top {
    align-items: flex-start;
  }
  .strong {
    font-weight: 500;
  }
  .small {
    font-size: 12px;
  }
  select {
    height: 32px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    min-width: 150px;
  }
  .segmented {
    display: flex;
    padding: 3px;
    gap: 2px;
    border-radius: 8px;
    background: var(--chip);
  }
  .segmented button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
  }
  .segmented button.on {
    background: var(--surface);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
  }
  .switch {
    width: 40px;
    height: 20px;
    flex: none;
    margin-top: 2px;
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
  .confirm {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--amber-soft);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .btn.danger {
    border-color: var(--red);
    color: var(--red);
    background: transparent;
    font-weight: 600;
    flex: none;
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .small-btn {
    height: 28px;
    padding: 0 10px;
    font-size: 12px;
  }
  .history {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .history li {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) 56px auto;
    gap: 8px;
    padding: 6px 0;
    border-top: 1px solid var(--divider);
    font-size: 12px;
  }
</style>
