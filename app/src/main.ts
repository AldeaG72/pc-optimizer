import "@fontsource-variable/inter";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { isTauri } from "./lib/window";

// Inside the app the window has Windows' Mica material behind it.
if (isTauri()) document.documentElement.classList.add("mica");

const target = document.getElementById("app");
if (!target) throw new Error("no #app element");

export default mount(App, { target });
