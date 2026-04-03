/** @jsxImportSource @spiders-wm/sdk */

import type { LayoutContext } from "@spiders-wm/sdk/layout";

const DEFAULT_FRAME_WEIGHTS = [12, 8] as const;
const DEFAULT_STACK_WEIGHT = 8;

export default function layout(ctx: LayoutContext) {
  const stackCount = Math.max(ctx.windows.length - 1, 0);
  const frameWeights = splitWeights(ctx, "frame", DEFAULT_FRAME_WEIGHTS);
  const stackWeights = splitWeights(
    ctx,
    "stack",
    Array.from({ length: stackCount }, () => DEFAULT_STACK_WEIGHT),
  );

  return (
    <workspace id="frame" class="playground-workspace">
      <slot
        id="master"
        take={1}
        class={joinClasses("master-slot", growClass(frameWeights[0]))}
      />

      {ctx.windows.length > 1 ? (
        <group
          id="stack"
          class={joinClasses("stack-group", growClass(frameWeights[1]))}
        >
          {stackWeights.map((weight, index) => (
            <slot
              id={index === 0 ? "stack-slot" : undefined}
              take={1}
              class={joinClasses("stack-group__item", growClass(weight))}
            />
          ))}
        </group>
      ) : null}
    </workspace>
  );
}

function splitWeights(
  ctx: LayoutContext,
  nodeId: string,
  defaults: readonly number[],
) {
  const override = ctx.state?.layoutAdjustments?.splitWeightsByNodeId?.[nodeId];

  return defaults.map((fallback, index) =>
    clampWeight(override?.[index] ?? fallback),
  );
}

function clampWeight(value: number) {
  const rounded = Math.round(value);
  return Math.max(1, Math.min(24, Number.isFinite(rounded) ? rounded : 8));
}

function growClass(weight: number) {
  return `grow-${clampWeight(weight)}`;
}

function joinClasses(...values: Array<string | undefined>) {
  return values.filter(Boolean).join(" ");
}
