/** @jsxImportSource @spiders-wm/sdk */

import type { LayoutContext } from "@spiders-wm/sdk/layout";

export default function layout(ctx: LayoutContext) {
  return (
    <workspace id="frame" class="playground-workspace">
      <slot take={1} class="main-pane" />

      {ctx.windows.length > 1 ? (
        <group id="right-column">
          <slot take={1} class="top-pane" />

          {ctx.windows.length > 2 ? (
            <group id="bottom-row">
              <slot take={1} class="bottom-pane" />

              {ctx.windows.length > 3 ? <slot class="bottom-pane" /> : null}
            </group>
          ) : null}
        </group>
      ) : null}
    </workspace>
  );
}
