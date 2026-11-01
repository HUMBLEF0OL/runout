import React from "react";
import ReactDOM from "react-dom/client";
import { domAnimation, LazyMotion, MotionConfig } from "motion/react";
import App from "./App";
import { loadMocks, MOCK } from "./api";
import "./styles/index.css";

// A failed mock load still renders: every read then shows its inline error instead of a blank window.
(MOCK ? loadMocks() : Promise.resolve()).catch(() => {}).then(() =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {/* Motion follows the OS reduced-motion setting (DESIGN §3; CSS motion has its own rule in tokens.css). LazyMotion + `m`
          keeps the bundle to the animation features used (no layout or drag); `strict` rejects a full `motion.*` import. */}
      <LazyMotion features={domAnimation} strict><MotionConfig reducedMotion="user"><App /></MotionConfig></LazyMotion>
    </React.StrictMode>,
  ),
);
