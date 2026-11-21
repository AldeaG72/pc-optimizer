<script lang="ts">
  import Sidebar from "./components/Sidebar.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Tour from "./components/Tour.svelte";
  import Home from "./pages/Home.svelte";
  import Settings from "./pages/Settings.svelte";
  import Tweaks from "./pages/Tweaks.svelte";
  import { loadSettings, persist, ui } from "./lib/state.svelte";

  $effect(() => {
    void loadSettings();
  });

  $effect(() => {
    const root = document.documentElement;
    root.lang = ui.lang;
    if (ui.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = ui.theme;
  });

  // Save whenever a saved choice changes (after the first load).
  $effect(() => {
    void [ui.lang, ui.theme, ui.collapsed];
    if (ui.loaded) void persist();
  });
</script>

<div class="window">
  <TitleBar />
  {#if !ui.loaded}
    <div class="body"></div>
  {:else if ui.tourOpen}
    <Tour />
  {:else}
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
  {/if}
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
