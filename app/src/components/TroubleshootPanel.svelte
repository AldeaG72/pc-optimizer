<script lang="ts">
  // Settings › Troubleshooting (SPEC 4.8): fixed repairs run by the helper (each asks first),
  // the shader cache, and shortcuts to Windows panels.
  import Loader from "@lucide/svelte/icons/loader";
  import { onMount } from "svelte";
  import {
    cleanShaderCache,
    openPanel,
    restart,
    runHelper,
    shaderCacheSize,
    type Panel,
    type RepairAction,
  } from "../lib/backend";
  import { formatBytes, helperError } from "../lib/describe";
  import { t, ui } from "../lib/state.svelte";

  const repairs: RepairAction[] = ["network_reset", "update_repair", "system_scan"];
  const panels: Panel[] = ["sound", "power", "network"];

  let asking = $state<RepairAction | "shaders" | null>(null);
  let running = $state<RepairAction | "shaders" | null>(null);
  let result = $state<Record<string, string>>({});
  let needReboot = $state(false);
  let shaders = $state<number | null | undefined>(undefined);

  onMount(async () => {
    try {
      shaders = await shaderCacheSize();
    } catch {
      shaders = null;
    }
  });

  async function repair(action: RepairAction) {
    asking = null;
    running = action;
    let text = "";
    try {
      await runHelper({ op: "repair", action }, (e) => {
        if (e.event === "result") {
          text = e.outcome.code === "repaired" ? t("app.repair.ok") : e.outcome.code === "failed" ? t("app.repair.failed", { error: e.outcome.error }) : "";
        } else if (e.event === "blocked") {
          text = t("app.repair.blocked");
        } else if (e.event === "done" && e.restart === "reboot" && text === t("app.repair.ok")) {
          needReboot = true;
        } else if (e.event === "error" || e.event === "refused") {
          text = t("app.repair.blocked");
        }
      });
    } catch (e) {
      text = String(e) === "cancelled" ? t("app.repair.uac_no") : helperError(e);
    }
    result = { ...result, [action]: text };
    running = null;
  }

  async function cleanShaders() {
    asking = null;
    running = "shaders";
    try {
      const freed = await cleanShaderCache();
      result = { ...result, shaders: t("app.repair.shaders_done", { size: formatBytes(freed, ui.lang) }) };
      shaders = await shaderCacheSize();
    } catch (e) {
      result = { ...result, shaders: String(e) };
    }
    running = null;
  }
</script>

<section class="card" aria-labelledby="repair">
  <h2 id="repair">{t("app.repair.title")}</h2>
  <p class="muted small">{t("app.repair.intro")}</p>

  {#each repairs as action (action)}
    <div class="row top">
      <div>
        <div class="strong">{t(`app.repair.${action}`)}</div>
        <div class="muted small">{result[action] ?? t(`app.repair.${action}_hint`)}</div>
      </div>
      <button class="btn small-btn" disabled={running !== null} onclick={() => (asking = action)}>
        {#if running === action}<Loader size={14} aria-hidden="true" />{/if}
        {t("app.repair.run")}
      </button>
    </div>
    {#if asking === action}
      <div class="confirm" role="alertdialog" aria-label={t(`app.repair.${action}`)}>
        <p>{t(`app.repair.${action}_confirm`)}</p>
        <div class="buttons">
          <button class="btn" onclick={() => (asking = null)}>{t("app.settings.cancel")}</button>
          <button class="btn primary" onclick={() => void repair(action)}>{t("app.repair.yes")}</button>
        </div>
      </div>
    {/if}
  {/each}

  {#if needReboot}
    <div class="row">
      <span>{t("app.repair.reboot")}</span>
      <button class="btn small-btn" onclick={() => void restart("reboot")}>{t("app.repair.reboot_now")}</button>
    </div>
  {/if}

  <div class="row top">
    <div>
      <div class="strong">{t("app.repair.shaders")}</div>
      <div class="muted small">
        {result.shaders ??
          (shaders === undefined
            ? t("app.clean.measuring")
            : t("app.repair.shaders_hint", { size: formatBytes(shaders ?? 0, ui.lang) }))}
      </div>
    </div>
    <button class="btn small-btn" disabled={running !== null || !shaders} onclick={() => (asking = "shaders")}>{t("app.clean.button")}</button>
  </div>
  {#if asking === "shaders"}
    <div class="confirm" role="alertdialog" aria-label={t("app.repair.shaders")}>
      <p>{t("app.repair.shaders_confirm")}</p>
      <div class="buttons">
        <button class="btn" onclick={() => (asking = null)}>{t("app.settings.cancel")}</button>
        <button class="btn primary" onclick={() => void cleanShaders()}>{t("app.clean.do")}</button>
      </div>
    </div>
  {/if}

  <div class="row">
    <span class="strong">{t("app.repair.panels")}</span>
    <div class="panels">
      {#each panels as p (p)}
        <button class="btn small-btn" onclick={() => void openPanel(p)}>{t(`app.repair.panel_${p}`)}</button>
      {/each}
    </div>
  </div>
</section>

<style>
  .card {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .small {
    font-size: 12px;
  }
  .strong {
    font-weight: 600;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .row.top {
    align-items: flex-start;
  }
  .small-btn {
    height: 30px;
    flex: none;
  }
  .confirm {
    padding: 12px;
    border-radius: 8px;
    background: var(--amber-soft);
  }
  .confirm p {
    margin: 0 0 8px;
  }
  .buttons,
  .panels {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .btn:disabled {
    opacity: 0.5;
  }
</style>
