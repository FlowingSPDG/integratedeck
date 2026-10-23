import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { installNativeFeel } from "./native-feel";
import "./styles.css";

installNativeFeel();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
