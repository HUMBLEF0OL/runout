// SC-01 (R2+) or SC-02 (MVP) by release.
import { useRelease } from "../hooks/useRelease";
import HomeFull from "./HomeFull";
import HomeMvp from "./HomeMvp";

export default function Home() {
  return useRelease().r2 ? <HomeFull /> : <HomeMvp />;
}
