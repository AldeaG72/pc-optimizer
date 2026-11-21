<script lang="ts">
  import Sidebar from "./components/Sidebar.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Home from "./pages/Home.svelte";
  import Settings from "./pages/Settings.svelte";
  import Tweaks from "./pages/Tweaks.svelte";
  import { ui } from "./lib/state.svelte";

  $effect(() => {
    const root = document.documentElement;
    root.lang = ui.lang;
    if (ui.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = ui.theme;
  });
</script>

<div class="window">
  <TitleBar />
  <div class="body">
    <Sidebar />
    <main>
      {#if ui.page === "home"}
        <Home />
      {:else if ui.page === "tweaks"}
        <Tweaks />
      {:else}
        <Settings />
      {/if}
    </main>
  </div>
</div>

<style>
  .window {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  main {
    flex: 1;
    min-width: 0;
    background: var(--content);
    border-top-left-radius: 10px;
    border-top: 1px solid var(--border);
    border-left: 1px solid var(--border);
    padding: 24px 28px;
    overflow: auto;
    user-select: text;
  }
</style>
