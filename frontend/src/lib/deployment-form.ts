import { z } from "zod"

import type { ExposedPort, PortKind } from "@/lib/types"

export const MEM_MIN_GB = 32
export const MEM_MAX_GB = 256
export const MEM_STEP_GB = 8
export const MEM_UNLIMITED_GB = MEM_MAX_GB + MEM_STEP_GB
export const MEM_PRESET_GB = 128

const portRow = z.object({
  container_port: z
    .string()
    .regex(/^\d+$/, "number required")
    .refine((value) => +value >= 1 && +value <= 65535, "1–65535"),
  kind: z.enum(["HTTP", "HTTPS", "TCP", "UDP"]),
  host_port: z
    .string()
    .regex(/^\d*$/, "number")
    .refine((value) => value === "" || (+value >= 20000 && +value <= 29999), "20000–29999"),
})

const envRow = z.object({
  key: z
    .string()
    .regex(/^[A-Za-z_][A-Za-z0-9_]*$/, "letters/digits/underscore")
    .max(128),
  value: z.string().max(4096),
  is_secret: z.boolean(),
})

const volumeRow = z.object({
  volume_id: z.string().nullable().optional(),
  volume_name: z
    .string()
    .regex(/^[A-Za-z0-9][A-Za-z0-9_-]*$/, "alphanumeric/dash/underscore")
    .max(63),
  container_path: z.string().startsWith("/", "must be absolute").max(255),
  read_only: z.boolean(),
  placement: z.enum(["SLOT", "SERVER"]),
  purge_on_redeploy: z.boolean(),
})

export const deploymentFormSchema = z.object({
  name: z
    .string()
    .regex(/^$|^[A-Za-z0-9][A-Za-z0-9_-]*$/, "alphanumeric/dash/underscore")
    .max(63),
  ports: z.array(portRow).max(32),
  env: z.array(envRow).max(64),
  volumes: z.array(volumeRow).max(16),
  mem_limit_gb: z.number().min(MEM_MIN_GB).max(MEM_UNLIMITED_GB),
})

export type DeploymentFormValues = z.infer<typeof deploymentFormSchema>

const WEB_PORTS = new Set([80, 3000, 5000, 7860, 8000, 8080, 8081, 8188, 8501, 8888])

export function defaultPortKind(port: ExposedPort, appsEnabled: boolean): PortKind {
  if (port.protocol === "udp") return "UDP"
  if (!appsEnabled) return "TCP"
  if (port.container_port === 443 || port.container_port === 8443) return "HTTPS"
  return WEB_PORTS.has(port.container_port) ? "HTTP" : "TCP"
}
