import { useToasts } from "../toast";

export function Toasts() {
  const toasts = useToasts();
  return (
    <div class="toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} class="toast">
          {t.message}
        </div>
      ))}
    </div>
  );
}
