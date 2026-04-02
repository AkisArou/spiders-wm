/** @jsxImportSource ../../layout-runtime */

import type { LayoutContext } from "@spiders-wm/sdk/layout";

export default function layout(ctx: LayoutContext) {
  const showStack = ctx.windows.length > 2;

  return (
    <workspace id="root" class="playground-workspace">
      <group id="frame" class="stack-frame">
        <group id="primary-column" class="column column--primary">
          <window
            id="main-terminal"
            class="pane pane--main"
            match={'app_id="foot" title="Terminal 1"'}
          />
          <slot id="primary-fill" take={1} class="pane pane--support" />
        </group>

        {showStack ? (
          <group id="stack" class="column column--stack">
            <slot class="pane pane--stack" />
          </group>
        ) : null}
      </group>
    </workspace>
  );
}
