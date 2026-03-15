import type { SlotProps } from "spiders-wm/layout";

export function StackSlot(props: SlotProps) {
  return <slot class="stack-group__item" {...props} />;
}
