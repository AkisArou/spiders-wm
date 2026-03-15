import type { LayoutContext } from "spiders-wm/layout"

import { MasterSlot } from "./components/MasterSlot"
import { StackGroup } from "../../components/StackGroup"
import "./index.css"

export default function layout(ctx: LayoutContext) {
  return (
    <workspace id="root">
      <group id="frame">
        <MasterSlot />
        <StackGroup ctx={ctx} />
      </group>
    </workspace>
  );
}
