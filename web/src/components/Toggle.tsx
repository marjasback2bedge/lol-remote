type Props = { checked: boolean; onChange: (v: boolean) => void; label: string; hint?: string };

export function Toggle({ checked, onChange, label, hint }: Props) {
  return (
    <label class="toggle">
      <span>
        <span class="toggle-label">{label}</span>
        {hint && <span class="toggle-hint">{hint}</span>}
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={checked}
        onChange={(e) => onChange((e.target as HTMLInputElement).checked)}
      />
      <span class="toggle-track" aria-hidden="true" />
    </label>
  );
}
