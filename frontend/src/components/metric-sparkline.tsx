import { Area, AreaChart, YAxis } from "recharts"

import { ChartContainer, ChartTooltip, ChartTooltipContent } from "@/components/ui/chart"

interface Point {
  ts: string
  value: number
}

/** Compact area sparkline for the server page metric cards. */
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
  return (
    <ChartContainer
      config={{ value: { label, color: "var(--slot-running)" } }}
      className="h-16 w-full"
    >
      <AreaChart data={points} margin={{ top: 2, right: 0, bottom: 0, left: 0 }}>
        <YAxis domain={[0, max ?? "auto"]} hide />
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
        />
      </AreaChart>
    </ChartContainer>
  )
}
