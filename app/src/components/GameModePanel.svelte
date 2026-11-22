<script lang="ts">
  // Settings › Game mode (SPEC 4.5): on/off, what happens while a game runs, extra games,
  // and "Start with Windows" (so it works without opening the app).
  import X from "@lucide/svelte/icons/x";
  import { onMount } from "svelte";
  import { gamemodeSuggested } from "../lib/backend";
  import { persist, prefs, t } from "../lib/state.svelte";

  let suggested = $state<string[]>([]);
  let newGame = $state("");
  let newApp = $state("");

  onMount(async () => {
    suggested = await gamemodeSuggested();
  });

  const apps = $derived([...new Set([...suggested, ...prefs.gameMode.closeApps])]);
  const isExe = (s: string) => /^[A-Za-z0-9 ._()+-]{1,76}\.exe$/i.test(s.trim());

  function save() {
    void persist();
  }

  function toggle(key: "enabled" | "highPerformance") {
    prefs.gameMode = { ...prefs.gameMode, [key]: !prefs.gameMode[key] };
    save();
  }

  function flipApp(name: string) {
    const list = prefs.gameMode.closeApps;
    prefs.gameMode = {
      ...prefs.gameMode,
      closeApps: list.includes(name) ? list.filter((x) => x !== name) : [...list, name],
    };
    save();
  }

  function addApp() {
    const name = newApp.trim().toLowerCase();
    if (!isExe(name)) return;
    if (!prefs.gameMode.closeApps.includes(name)) {
      prefs.gameMode = { ...prefs.gameMode, closeApps: [...prefs.gameMode.closeApps, name] };
      save();
    }
    newApp = "";
  }

  function addGame() {
    const name = newGame.trim().toLowerCase();
    if (!isExe(name)) return;
    if (!prefs.gameMode.extraGames.includes(name)) {
      prefs.gameMode = { ...prefs.gameMode, extraGames: [...prefs.gameMode.extraGames, name] };
      save();
    }
    newGame = "";
  }

  function removeGame(name: string) {
    prefs.gameMode = { ...prefs.gameMode, extraGames: prefs.gameMode.extraGames.filter((g) => g !== name) };
    save();
  }

  function toggleAutostart() {
    prefs.startWithWindows = !prefs.startWithWindows;
    save();
  }
</script>

<section class="card" aria-labelledby="gamemode">
  <div class="row top">
    <div>
      <h2 id="gamemode">{t("app.gamemode.title")}</h2>
      <div class="muted small">{t("app.gamemode.hint")}</div>
    </div>
    <button class="switch" class:on={prefs.gameMode.enabled} role="switch" aria-checked={prefs.gameMode.enabled} aria-label={t("app.gamemode.title")} onclick={() => toggle("enabled")}><span></span></button>
  </div>

  {#if prefs.gameMode.enabled}
    <div class="row top">
      <div>
        <div class="strong">{t("app.gamemode.power")}</div>
        <div class="muted small">{t("app.gamemode.power_hint")}</div>
      </div>
      <button class="switch" class:on={prefs.gameMode.highPerformance} role="switch" aria-checked={prefs.gameMode.highPerformance} aria-label={t("app.gamemode.power")} onclick={() => toggle("highPerformance")}><span></span></button>
    </div>

    <div>
      <div class="strong">{t("app.gamemode.close")}</div>
      <div class="muted small">{t("app.gamemode.close_hint")}</div>
      <ul class="apps">
        {#each apps as name (name)}
          <li>
            <label><input type="checkbox" checked={prefs.gameMode.closeApps.includes(name)} onchange={() => flipApp(name)} /> {name}</label>
          </li>
        {/each}
      </ul>
      <form class="add" onsubmit={(e) => { e.preventDefault(); addApp(); }}>
        <input placeholder="app.exe" aria-label={t("app.gamemode.add_app")} bind:value={newApp} />
        <button class="btn small-btn" disabled={!isExe(newApp)}>{t("app.gamemode.add")}</button>
      </form>
    </div>

    <div>
      <div class="strong">{t("app.gamemode.games")}</div>
      <div class="muted small">{t("app.gamemode.games_hint")}</div>
      {#each prefs.gameMode.extraGames as g (g)}
        <div class="chip">{g} <button class="icon-btn" aria-label={t("app.gamemode.remove", { name: g })} onclick={() => removeGame(g)}><X size={12} /></button></div>
      {/each}
      <form class="add" onsubmit={(e) => { e.preventDefault(); addGame(); }}>
        <input placeholder="game.exe" aria-label={t("app.gamemode.add_game")} bind:value={newGame} />
        <button class="btn small-btn" disabled={!isExe(newGame)}>{t("app.gamemode.add")}</button>
      </form>
    </div>
  {/if}

  <div class="row top">
    <div>
      <div class="strong">{t("app.gamemode.autostart")}</div>
      <div class="muted small">{t("app.gamemode.autostart_hint")}</div>
    </div>
    <button class="switch" class:on={prefs.startWithWindows} role="switch" aria-checked={prefs.startWithWindows} aria-label={t("app.gamemode.autostart")} onclick={toggleAutostart}><span></span></button>
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
    gap: 12px;
  }
  .apps {
    list-style: none;
    padding: 0;
    margin: 6px 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px 12px;
  }
  .add {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .add input {
    flex: 1;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }
  .small-btn {
    height: 30px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin: 6px 6px 0 0;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--chip);
  }
  .icon-btn {
    border: 0;
    background: transparent;
    padding: 0;
    display: inline-flex;
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .switch {
    flex: none;
    width: 40px;
    height: 20px;
    border-radius: 999px;
    border: 1px solid var(--border);
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
</style>
