/** @jsxImportSource @spiders-wm/sdk */

import type { LayoutContext } from "@spiders-wm/sdk/layout";

export default function layout(ctx: LayoutContext) {
  return (
    <workspace id="frame" class="playground-workspace">
      <group id="main-column">
        <slot take={1} class="main-pane" />

        {ctx.windows.length > 1 ? (
          <slot take={1} class="main-pane" />
        ) : null}
      </group>

      {ctx.windows.length > 2 ? (
        <group id="side-column">
          <slot class="side-pane" />
        </group>
      ) : null}
    </workspace>
  );
}
