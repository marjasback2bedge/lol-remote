import type { ComponentChildren } from "preact";
import { ApiError } from "../api";
import { useBusy } from "../hooks";
import { toast } from "../toast";

type Props = {
  onClick: () => Promise<unknown>;
  class?: string;
  disabled?: boolean;
  children: ComponentChildren;
};

/** A button that shows a spinner while its request runs and toasts failures. */
export function ActionButton({ onClick, class: cls = "", disabled, children }: Props) {
  const [busy, run] = useBusy();
  const click = () =>
    run(async () => {
      try {
        await onClick();
      } catch (e) {
        toast(e instanceof ApiError ? e.message : "操作失敗");
      }
    });
  return (
    <button class={`btn ${cls}`} disabled={disabled || busy} aria-busy={busy} onClick={click}>
      {busy ? <span class="spinner" aria-label="處理中" /> : children}
    </button>
  );
}
