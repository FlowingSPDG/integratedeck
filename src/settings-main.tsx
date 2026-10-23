import React from "react";
import ReactDOM from "react-dom/client";
import SettingsApp from "./SettingsApp";
import { installNativeFeel } from "./native-feel";
import "./settings.css";

installNativeFeel();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SettingsApp />
  </React.StrictMode>,
);
