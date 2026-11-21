<script lang="ts">
  import House from "@lucide/svelte/icons/house";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import Settings from "@lucide/svelte/icons/settings";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import { t, ui, type Page } from "../lib/state.svelte";

  const items: { page: Page; key: string; icon: typeof House }[] = [
    { page: "home", key: "app.nav.home", icon: House },
    { page: "tweaks", key: "app.nav.tweaks", icon: SlidersHorizontal },
    { page: "settings", key: "app.nav.settings", icon: Settings },
  ];
  // The Apply page belongs to Tweaks.
  const active = (page: Page) => ui.page === page || (page === "tweaks" && ui.page === "apply");
</script>

<nav aria-label={t("app.nav.label")} class:collapsed={ui.collapsed}>
  <div class="items">
    {#each items as item (item.page)}
      {@const Icon = item.icon}
      <button
        class="item"
        class:active={active(item.page)}
        aria-current={active(item.page) ? "page" : undefined}
        aria-label={ui.collapsed ? t(item.key) : undefined}
        title={ui.collapsed ? t(item.key) : undefined}
        onclick={() => (ui.page = item.page)}
      >
        <span class="mark" aria-hidden="true"></span>
        <Icon size={18} strokeWidth={1.8} aria-hidden="true" />
        {#if !ui.collapsed}<span class="label">{t(item.key)}</span>{/if}
      </button>
    {/each}
  </div>
  <button
    class="toggle"
    aria-label={t(ui.collapsed ? "app.nav.expand" : "app.nav.collapse")}
    aria-expanded={!ui.collapsed}
    onclick={() => (ui.collapsed = !ui.collapsed)}
  >
    <PanelLeft size={18} strokeWidth={1.8} aria-hidden="true" />
  </button>
</nav>

<style>
  nav {
    width: 216px;
    flex: none;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 8px 10px 14px;
  }
  nav.collapsed {
    width: 64px;
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 40px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
  }
  .item:hover {
    background: var(--nav-active);
  }
  .item.active {
    background: var(--nav-active);
    font-weight: 600;
  }
  .mark {
    width: 3px;
    height: 16px;
    border-radius: 3px;
    margin-left: -8px;
    margin-right: -7px;
    background: transparent;
  }
  .active .mark {
    background: var(--accent);
  }
  .toggle {
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-left: 2px;
  }
  .toggle:hover {
    background: var(--nav-active);
  }
</style>
