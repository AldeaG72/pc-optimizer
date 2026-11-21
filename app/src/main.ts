import "@fontsource-variable/inter";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { isTauri } from "./lib/window";
import { systemLang } from "./lib/backend";
import { ui } from "./lib/state.svelte";

// Inside the app the window has Windows' Mica material behind it, and the language is the
// Windows display language.
if (isTauri()) {
  document.documentElement.classList.add("mica");
  void systemLang().then((lang) => {
    if (lang) ui.lang = lang;
  });
}

const target = document.getElementById("app");
if (!target) throw new Error("no #app element");

export default mount(App, { target });
