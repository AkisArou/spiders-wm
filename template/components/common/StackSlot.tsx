import type { SlotProps } from "spider-wm/layout";

export function StackSlot(props: SlotProps) {
  return <slot class="stack-group__item" {...props} />;
}
