import { Area, AreaChart, YAxis } from "recharts"

import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart"

interface Point {
  ts: string
  value: number
}

/** Compact area sparkline for metric cards.
 *
 * Scale rule (operator requirement): percentage series are always
 * 0–100; series with a known capacity (memory, disk) are 0–capacity.
 * Only unbounded series (network rates, power without a limit) may
 * auto-scale. Pass `max` whenever the upper limit is known. */
export function MetricSparkline({
  points,
  label,
  unit,
  max,
}: {
  points: Point[]
  label: string
  unit: string
  max?: number
}) {
  // Guard degenerate capacities (0/NaN would collapse the domain).
  const upper = max !== undefined && Number.isFinite(max) && max > 0 ? max : undefined

  return (
    <ChartContainer
      config={{ value: { label, color: "var(--slot-running)" } }}
      className="h-16 w-full"
    >
      <AreaChart data={points} margin={{ top: 2, right: 0, bottom: 0, left: 0 }}>
        <YAxis
          hide
          type="number"
          domain={upper !== undefined ? [0, upper] : [0, "auto"]}
          allowDataOverflow
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(_, payload) => {
                const ts = payload?.[0]?.payload?.ts
                return ts ? new Date(ts).toLocaleTimeString() : ""
              }}
              formatter={(value) => `${Number(value).toFixed(1)} ${unit}`}
            />
          }
        />
        <Area
          dataKey="value"
          type="monotone"
          stroke="var(--color-value)"
          fill="var(--color-value)"
          fillOpacity={0.15}
          strokeWidth={1.5}
          isAnimationActive={false}
          baseValue={0}
        />
      </AreaChart>
    </ChartContainer>
  )
}
