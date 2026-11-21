<script lang="ts">
  import Gauge from "@lucide/svelte/icons/gauge";
  import Languages from "@lucide/svelte/icons/languages";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import { acceptTour, t, ui } from "../lib/state.svelte";
  import type { Lang } from "../lib/i18n";

  const TOTAL = 4;
  let step = $state(0);
  const icons = [Languages, Gauge, RotateCcw, ShieldCheck];
  const Icon = $derived(icons[step] ?? Gauge);
  const langs: { value: Lang; label: string }[] = [
    { value: "ro", label: "Română" },
    { value: "en", label: "English" },
  ];

  function next() {
    if (step < TOTAL - 1) step += 1;
    else void acceptTour();
  }
</script>

<div class="wrap">
  <section class="card tour" aria-live="polite" aria-label={t("app.tour.step", { n: step + 1, total: TOTAL })}>
    <div class="icon" aria-hidden="true"><Icon size={36} strokeWidth={1.7} /></div>

    {#if step === 0}
      <h1>{t("app.tour.lang_title")}</h1>
      <p class="muted">{t("app.tour.lang_text")}</p>
      <div class="langs" role="radiogroup" aria-label={t("app.settings.language")}>
        {#each langs as l (l.value)}
          <button role="radio" aria-checked={ui.lang === l.value} class:on={ui.lang === l.value} onclick={() => (ui.lang = l.value)}
            >{l.label}</button
          >
        {/each}
      </div>
    {:else if step === 1}
      <h1>{t("app.tour.does_title")}</h1>
      <div class="cols">
        <div class="col">
          <div class="head does">{t("app.tour.does")}</div>
          <p>{t("app.tour.does_1")}</p>
          <p>{t("app.tour.does_2")}</p>
          <p>{t("app.tour.does_3")}</p>
        </div>
        <div class="col">
          <div class="head doesnt">{t("app.tour.doesnt")}</div>
          <p>{t("app.tour.doesnt_1")}</p>
          <p>{t("app.tour.doesnt_2")}</p>
          <p>{t("app.tour.doesnt_3")}</p>
        </div>
      </div>
    {:else if step === 2}
      <h1>{t("app.tour.undo_title")}</h1>
      <p class="muted narrow">{t("app.tour.undo_1")}</p>
      <p class="muted narrow">{t("app.tour.undo_2")}</p>
    {:else}
      <h1>{t("app.tour.bench_title")}</h1>
      <p class="muted narrow">{t("app.tour.bench_text")}</p>
      <div class="disclaimer">{t("app.disclaimer")} {t("app.tour.disclaimer_where")}</div>
    {/if}

    <div class="spacer"></div>
    <div class="dots" aria-hidden="true">
      {#each Array.from({ length: TOTAL }, (_, i) => i) as i (i)}
        <span class:current={i === step}></span>
      {/each}
    </div>
    <div class="nav">
      <button class="btn" style:visibility={step === 0 ? "hidden" : "visible"} onclick={() => (step -= 1)}>{t("app.tour.back")}</button>
      <button class="btn primary" onclick={next}>{t(step === TOTAL - 1 ? "app.tour.agree" : "app.tour.next")}</button>
    </div>
  </section>
</div>

<style>
  .wrap {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }
  .tour {
    width: 600px;
    min-height: 500px;
    padding: 40px 48px 28px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 14px;
    font-size: 14px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.08);
  }
  .icon {
    width: 72px;
    height: 72px;
    border-radius: 18px;
    background: var(--accent-soft);
    color: var(--accent-text);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  h1 {
    font-size: 24px;
    margin-top: 6px;
  }
  p {
    margin: 0;
  }
  .narrow {
    max-width: 470px;
  }
  .langs {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 320px;
    margin-top: 10px;
  }
  .langs button {
    height: 48px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    font-weight: 600;
  }
  .langs button.on {
    border: 2px solid var(--accent);
    background: var(--accent-soft);
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    width: 100%;
    text-align: left;
    margin-top: 6px;
    font-size: 13px;
  }
  .col {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head {
    font-weight: 600;
  }
  .does {
    color: var(--green);
  }
  .doesnt {
    color: var(--red);
  }
  .disclaimer {
    text-align: left;
    font-size: 13px;
    background: var(--chip);
    border-radius: 10px;
    padding: 12px 14px;
    max-width: 470px;
  }
  .spacer {
    flex: 1;
  }
  .dots {
    display: flex;
    gap: 8px;
  }
  .dots span {
    width: 8px;
    height: 8px;
    border-radius: 99px;
    background: var(--border);
  }
  .dots span.current {
    width: 22px;
    background: var(--accent);
  }
  .nav {
    display: flex;
    justify-content: space-between;
    width: 100%;
  }
  .nav .btn {
    height: 38px;
  }
</style>
