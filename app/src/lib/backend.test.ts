import { describe, expect, it } from "vitest";
import { recipes, runHelper, systemLang, type HelperEvent } from "./backend";

// Outside the app the bridge answers with sample data and never touches the PC.
describe("backend outside the app", () => {
  it("lists sample recipes in the chosen language", async () => {
    const ro = (await recipes("ro")).recipes;
    const en = (await recipes("en")).recipes;
    expect(ro.map((r) => r.id)).toEqual(en.map((r) => r.id));
    expect(ro[0]?.name).toBe("Captură în fundal (Game DVR) oprită");
    expect(en[0]?.name).toBe("Background capture (Game DVR) off");
    expect(await systemLang()).toBeNull();
  });

  it("streams helper events in order, ending with one final event", async () => {
    const events: HelperEvent[] = [];
    await runHelper({ op: "apply", ids: ["gaming.game-dvr-off"] }, (e) => events.push(e));
    expect(events.map((e) => e.event)).toEqual(["hello", "started", "result", "done"]);
    const result = events[2];
    expect(result?.event === "result" && result.outcome.code).toBe("applied");
  });
});
