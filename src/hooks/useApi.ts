import { useCallback, useEffect, useRef, useState } from "react";
import { getOnline, onEvent, onInvalidate, type AppError } from "../api";

export interface ApiState<T> { data: T | undefined; error: AppError | undefined; loading: boolean; reload: () => void }

/** Run a read command, refetch whenever data changes (any write command, dev state switch). */
export function useApi<T>(fn: () => Promise<T>, deps: unknown[] = []): ApiState<T> {
  const [state, setState] = useState<{ data?: T; error?: AppError; loading: boolean }>({ loading: true });
  const seq = useRef(0);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const run = useCallback(() => {
    const n = ++seq.current;
    setState((s) => ({ ...s, loading: true }));
    fn().then(
      (data) => { if (n === seq.current) setState({ data, loading: false }); },
      (error: AppError) => { if (n === seq.current) setState({ error, loading: false }); },
    );
  }, deps);
  useEffect(() => { run(); return onInvalidate(run); }, [run]);
  return { data: state.data, error: state.error, loading: state.loading && state.data === undefined, reload: run };
}

/** Online state from `get_online` + `online-changed` (TSD §4.3). Embeds and short links depend on it. */
export function useOnline(): boolean {
  const { data } = useApi(getOnline);
  const [live, setLive] = useState<boolean | undefined>();
  useEffect(() => onEvent("online-changed", setLive), []);
  return live ?? data ?? false; // unknown = offline: no embed mounts before get_online answers (TSD §4.3)
}
