import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import StartupPanel from "./StartupPanel.svelte";
import { ui } from "../lib/state.svelte";

beforeEach(() => {
  ui.lang = "ro";
  ui.loaded = true;
});
afterEach(cleanup);

describe("startup programs", () => {
  it("shows what each is, the impact and the advice, and applies flips together", async () => {
    render(StartupPanel);
    expect(await screen.findByText("OneDrive: sincronizează fișierele cu cloud-ul")).toBeTruthy();
    expect(screen.getAllByText("Impact mare").length).toBe(2);
    expect(screen.getByText("Lasă pornit")).toBeTruthy();

    const apply = screen.getByRole("button", { name: /Aplică/ }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    const onedrive = screen.getByRole("switch", { name: "OneDrive" });
    expect(onedrive.getAttribute("aria-checked")).toBe("true");
    await fireEvent.click(onedrive);
    expect(onedrive.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("button", { name: "Aplică 1 schimbare" })).toBeTruthy();
    // Flipping back cancels the change.
    await fireEvent.click(onedrive);
    expect((screen.getByRole("button", { name: /Aplică/ }) as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(screen.getByRole("switch", { name: "Steam" }));
    await fireEvent.click(screen.getByRole("button", { name: "Aplică 1 schimbare" }));
    expect(await screen.findByText("Gata: 1 schimbare făcută.")).toBeTruthy();
  });
});
