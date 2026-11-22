import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import Settings from "./Settings.svelte";
import { prefs, ui } from "../lib/state.svelte";
import { defaultSettings, memory } from "../lib/backend";
import { catalog, clearPending, loadCatalog, pending } from "../lib/tweaks.svelte";
import { sampleBench } from "../lib/sample";

beforeEach(() => {
  memory.settings = { ...defaultSettings(), lang: "ro", tourAccepted: 1 };
  Object.assign(prefs, memory.settings);
  ui.lang = "ro";
  ui.loaded = true;
  ui.page = "settings";
  clearPending();
  sampleBench.before = null;
  sampleBench.after = null;
});
afterEach(cleanup);

describe("Settings", () => {
  it("the restore point switch is saved", async () => {
    render(Settings);
    const sw = await screen.findByRole("switch", { name: "Punct de restaurare înainte de aplicare" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    await fireEvent.click(sw);
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(memory.settings.restorePoint).toBe(true);
  });

  it("advanced tweaks need a confirmation", async () => {
    render(Settings);
    await fireEvent.click(await screen.findByRole("switch", { name: "Afișează tweak-urile avansate" }));
    expect(prefs.showAdvanced).toBe(false);
    await fireEvent.click(screen.getByRole("button", { name: "Renunță" }));
    expect(prefs.showAdvanced).toBe(false);
    await fireEvent.click(screen.getByRole("switch", { name: "Afișează tweak-urile avansate" }));
    await fireEvent.click(screen.getByRole("button", { name: "Da, afișează-le" }));
    expect(prefs.showAdvanced).toBe(true);
    expect(memory.settings.showAdvanced).toBe(true);
  });

  it("'Undo all' asks, then undoes every applied tweak", async () => {
    await loadCatalog("ro");
    render(Settings);
    await screen.findByText("Nu e nimic de anulat.");
    await waitFor(() => expect(catalog.loading).toBe(false));
    const dvr = catalog.list.find((r) => r.id === "gaming.game-dvr-off")!;
    dvr.status = { code: "applied" };
    const button = await screen.findByRole("button", { name: "Anulează tot" });
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
    await fireEvent.click(button);
    expect(screen.getByText("Anulez 1 modificare?")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Da, anulează tot" }));
    expect(pending.undo).toEqual(["gaming.game-dvr-off"]);
    expect(ui.page).toBe("apply");
  });

  it("shows the benchmark history and deletes it on request", async () => {
    sampleBench.before = [140, 141, 142];
    render(Settings);
    expect(await screen.findByText("3 rulări, FPS 141,0")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Șterge istoricul" }));
    expect(screen.getByText("Șterg 3 măsurători? Nu se pot recupera.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Da, șterge" }));
    expect(await screen.findByText("Nicio măsurătoare încă.")).toBeTruthy();
    expect(sampleBench.before).toBeNull();
  });

  it("About has the version, the disclaimer and the licenses", async () => {
    render(Settings);
    expect(await screen.findByText("PC Optimizer 0.0.1 beta · GPL-3.0")).toBeTruthy();
    expect(screen.getByText(/pe propria răspundere/)).toBeTruthy();
    expect(screen.getByText(/PresentMon/)).toBeTruthy();
  });
});

describe("Safety (3.13)", () => {
  it("the daily backup switch is saved, and the log can be exported", async () => {
    render(Settings);
    const sw = await screen.findByRole("switch", { name: "Copie zilnică a registrului" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    await fireEvent.click(sw);
    await waitFor(() => expect(memory.settings.dailyBackup).toBe(true));

    await fireEvent.click(screen.getByRole("button", { name: "Exportă…" }));
    expect(await screen.findByText("Salvat: pc-optimizer-log.txt")).toBeTruthy();
  });
});

describe("Troubleshooting (3.14)", () => {
  it("asks before a repair, then reports it and offers the restart", async () => {
    render(Settings);
    const reset = (await screen.findByText("Resetare rețea")).closest(".row") as HTMLElement;
    await fireEvent.click(reset.querySelector("button") as HTMLButtonElement);
    expect(screen.getByText(/Resetez setările de rețea ale Windows\?/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Da, pornește" }));
    expect(await screen.findByText("Gata, fără erori.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Restart acum" })).toBeTruthy();
  });

  it("shows the shader cache size and empties it after asking", async () => {
    render(Settings);
    expect(await screen.findByText(/700 MB în cache-urile DirectX/)).toBeTruthy();
    const row = screen.getByText("Cache-ul de shadere").closest(".row") as HTMLElement;
    await fireEvent.click(row.querySelector("button") as HTMLButtonElement);
    await fireEvent.click(screen.getByRole("button", { name: "Da, șterge" }));
    expect(await screen.findByText("Gata: am eliberat 700 MB.")).toBeTruthy();
  });
});
