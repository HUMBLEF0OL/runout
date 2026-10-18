import { createHashRouter, Navigate, RouterProvider } from "react-router-dom";
import type { ComponentType } from "react";
import Shell from "./components/Shell";
import { ToastProvider } from "./components/ui";

// Each page is its own chunk, fetched on first visit; only the shell loads up front.
const page = (load: () => Promise<{ default: ComponentType }>) => async () => ({ Component: (await load()).default });

// Hash routes match the prototype (DESIGN §6).
const router = createHashRouter([
  { path: "/system", lazy: page(() => import("./pages/System")) },
  {
    element: <Shell />,
    children: [
      { path: "/home", lazy: page(() => import("./pages/Home")) },
      { path: "/first-run", lazy: page(() => import("./pages/FirstRun")) },
      { path: "/backlog", lazy: page(() => import("./pages/Backlog")) },
      { path: "/library/:type?", lazy: page(() => import("./pages/Library")) },
      { path: "/search/:q", lazy: page(() => import("./pages/Search")) },
      { path: "/item/:id", lazy: page(() => import("./pages/Item")) },
      { path: "/rediscover", lazy: page(() => import("./pages/Rediscover")) },
      { path: "/plays/:tab?", lazy: page(() => import("./pages/Plays")) },
      { path: "/insights/:tab?", lazy: page(() => import("./pages/Insights")) },
      { path: "/import/:step?", lazy: page(() => import("./pages/Import")) },
      { path: "/playlists/:tab?", lazy: page(() => import("./pages/Playlists")) },
      { path: "/spotify/:tab?", lazy: page(() => import("./pages/Spotify")) },
      { path: "/settings", lazy: page(() => import("./pages/Settings")) },
      { path: "/settings/tags", lazy: page(() => import("./pages/SettingsTags")) },
      { path: "*", element: <Navigate to="/home" replace /> },
    ],
  },
]);

export default function App() {
  return <ToastProvider><RouterProvider router={router} /></ToastProvider>;
}
