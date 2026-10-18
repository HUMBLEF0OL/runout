import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { loadMocks, MOCK } from "./api";
import "./styles/index.css";

// A failed mock load still renders: every read then shows its inline error instead of a blank window.
(MOCK ? loadMocks() : Promise.resolve()).catch(() => {}).then(() =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  ),
);
