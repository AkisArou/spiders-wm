/** @jsxImportSource @spiders-wm/sdk */

import type { LayoutContext } from "@spiders-wm/sdk/layout";

import { MasterSlot } from "./components/MasterSlot.tsx";
import { StackGroup } from "../../components/StackGroup.tsx";
import "./index.css";

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
