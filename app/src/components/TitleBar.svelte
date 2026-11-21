<script lang="ts">
  import Gauge from "@lucide/svelte/icons/gauge";
  import Minus from "@lucide/svelte/icons/minus";
  import Square from "@lucide/svelte/icons/square";
  import X from "@lucide/svelte/icons/x";
  import { t } from "../lib/state.svelte";
  import { close, minimize, toggleMaximize } from "../lib/window";
</script>

<header class="bar" data-tauri-drag-region>
  <div class="name" data-tauri-drag-region>
    <span class="logo" aria-hidden="true"><Gauge size={14} strokeWidth={2.4} /></span>
    <span class="title" data-tauri-drag-region>PC Optimizer</span>
    <span class="beta">{t("app.beta")}</span>
  </div>
  <div class="controls">
    <button aria-label={t("app.window.minimize")} onclick={minimize}><Minus size={14} strokeWidth={1.6} /></button>
    <button aria-label={t("app.window.maximize")} onclick={toggleMaximize}><Square size={12} strokeWidth={1.8} /></button>
    <button class="close" aria-label={t("app.window.close")} onclick={close}><X size={14} strokeWidth={1.6} /></button>
  </div>
</header>

<style>
  .bar {
    height: 40px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 14px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 100%;
  }
  .logo {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    background: var(--accent);
    color: var(--on-accent);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .title {
    font-weight: 600;
    font-size: 12px;
  }
  .beta {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 500;
  }
  .controls {
    display: flex;
    height: 40px;
  }
  .controls button {
    width: 46px;
    height: 40px;
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .controls button:hover {
    background: var(--nav-active);
  }
  .controls .close:hover {
    background: #c42b1c;
    color: #ffffff;
  }
</style>
