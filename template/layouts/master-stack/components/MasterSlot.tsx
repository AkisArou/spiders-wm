/** @jsxImportSource @spiders-wm/sdk */

import type { SlotProps } from "@spiders-wm/sdk/layout";

type MasterSlotProps = Omit<SlotProps, "id" | "class">;

export function MasterSlot({ take = 1, ...props }: MasterSlotProps) {
  return <slot id="master" take={take} class="master-slot" {...props} />;
}
