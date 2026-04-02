/** @jsxImportSource @spiders-wm/sdk */

import type { SlotProps } from "@spiders-wm/sdk/layout";

export function StackSlot(props: SlotProps) {
  return <slot class="stack-group__item" {...props} />;
}
