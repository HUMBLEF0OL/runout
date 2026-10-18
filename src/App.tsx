import { createHashRouter, Navigate, RouterProvider } from "react-router-dom";
import Shell from "./components/Shell";
import { ToastProvider } from "./components/ui";
import Home from "./pages/Home";
import FirstRun from "./pages/FirstRun";
import Backlog from "./pages/Backlog";
import Library from "./pages/Library";
import Search from "./pages/Search";
import Item from "./pages/Item";
import Rediscover from "./pages/Rediscover";
import Plays from "./pages/Plays";
import Insights from "./pages/Insights";
import Import from "./pages/Import";
import Playlists from "./pages/Playlists";
import Spotify from "./pages/Spotify";
import Settings from "./pages/Settings";
import SettingsTags from "./pages/SettingsTags";
import System from "./pages/System";

// Hash routes match the prototype (DESIGN §6).
const router = createHashRouter([
  { path: "/system", element: <System /> },
  {
    element: <Shell />,
    children: [
      { path: "/home", element: <Home /> },
      { path: "/first-run", element: <FirstRun /> },
      { path: "/backlog", element: <Backlog /> },
      { path: "/library/:type?", element: <Library /> },
      { path: "/search/:q", element: <Search /> },
      { path: "/item/:id", element: <Item /> },
      { path: "/rediscover", element: <Rediscover /> },
      { path: "/plays/:tab?", element: <Plays /> },
      { path: "/insights/:tab?", element: <Insights /> },
      { path: "/import/:step?", element: <Import /> },
      { path: "/playlists/:tab?", element: <Playlists /> },
      { path: "/spotify/:tab?", element: <Spotify /> },
      { path: "/settings", element: <Settings /> },
      { path: "/settings/tags", element: <SettingsTags /> },
      { path: "*", element: <Navigate to="/home" replace /> },
    ],
  },
]);

export default function App() {
  return <ToastProvider><RouterProvider router={router} /></ToastProvider>;
}
