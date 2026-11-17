import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { useAi } from "./store/ai";
import { subscribeSettings, useSettings } from "./store/settings";
import { useSystem } from "./store/system";
import "./styles.css";
import Widget from "./widget/Widget";

const isWidget = window.location.hash.startsWith("#/widget");
if (isWidget) document.documentElement.classList.add("widget");

// Boot the shared stores once per window.
void useSettings.getState().load();
subscribeSettings();
void useSystem.getState().init();
void useAi.getState().init();

// Block the default WebKit context menu outside editable fields (desktop-app feel).
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .selectable")) e.preventDefault();
});

createRoot(document.getElementById("root")!).render(<StrictMode>{isWidget ? <Widget /> : <App />}</StrictMode>);
