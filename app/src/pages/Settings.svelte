<script lang="ts">
  import { t, ui, type Theme } from "../lib/state.svelte";
  import type { Lang } from "../lib/i18n";

  const themes: { value: Theme; key: string }[] = [
    { value: "system", key: "app.settings.theme_system" },
    { value: "light", key: "app.settings.theme_light" },
    { value: "dark", key: "app.settings.theme_dark" },
  ];
</script>

<div class="page">
  <h1>{t("app.settings.title")}</h1>
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
          <button
            role="radio"
            aria-checked={ui.theme === th.value}
            class:on={ui.theme === th.value}
            onclick={() => (ui.theme = th.value)}>{t(th.key)}</button
          >
        {/each}
      </div>
    </div>
  </section>
  <p class="muted">{t("app.soon")}</p>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 520px;
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
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
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
  p {
    margin: 0;
  }
</style>
