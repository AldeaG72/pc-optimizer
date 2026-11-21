import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import CleanupPanel from "./CleanupPanel.svelte";
import { formatBytes } from "../lib/describe";
import { ui } from "../lib/state.svelte";
import { sampleRecipes } from "../lib/sample";

beforeEach(() => {
  ui.lang = "ro";
  ui.loaded = true;
});
afterEach(cleanup);

const cleanups = () => sampleRecipes("ro").filter((r) => r.cleanup);

describe("cleanup", () => {
  it("nothing is ticked, asks first, then reports what was freed", async () => {
    render(CleanupPanel, { props: { recipes: cleanups(), onSelect: () => {} } });
    expect(await screen.findByText("1,5 GB")).toBeTruthy();
    expect(screen.getByText("Mărimea se vede după curățare (folder de sistem)")).toBeTruthy();
    const clean = screen.getByRole("button", { name: "Curăță" }) as HTMLButtonElement;
    expect(clean.disabled).toBe(true);

    await fireEvent.click(screen.getByRole("checkbox", { name: /Fișierele tale temporare/ }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /Fișierele temporare ale Windows/ }));
    await fireEvent.click(clean);
    expect(screen.getByText("Șterg definitiv 2 categorii, cel puțin 1,5 GB?")).toBeTruthy();
    expect(screen.getByText(/Windows cere permisiunea de administrator/)).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: "Da, șterge" }));
    // 1.5 GB of the user's own files (app) + 256 MB in the system folder (helper).
    expect(await screen.findByText("Gata: am eliberat 1,8 GB.")).toBeTruthy();
  });

  it("can be cancelled at the question", async () => {
    render(CleanupPanel, { props: { recipes: cleanups(), onSelect: () => {} } });
    await fireEvent.click(await screen.findByRole("checkbox", { name: /Fișierele tale temporare/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Curăță" }));
    await fireEvent.click(screen.getByRole("button", { name: "Renunță" }));
    expect(screen.queryByText(/Gata/)).toBeNull();
  });

  it("formats sizes in both languages", () => {
    expect(formatBytes(512, "ro")).toBe("512 B");
    expect(formatBytes(1_610_612_736, "ro")).toBe("1,5 GB");
    expect(formatBytes(1_610_612_736, "en")).toBe("1.5 GB");
  });
});
