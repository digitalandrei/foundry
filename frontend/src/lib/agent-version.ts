export function agentVersionAtLeast(version: string | null, minimum: [number, number, number]): boolean {
  if (!version) return false
  const match = version.replace(/^v/, "").match(/^(\d+)\.(\d+)\.(\d+)/)
  if (!match) return false
  const current = match.slice(1).map(Number)
  for (let index = 0; index < minimum.length; index += 1) {
    if (current[index] !== minimum[index]) return current[index] > minimum[index]
  }
  return true
}

export function agentSupportsOperationalSafety(version: string | null): boolean {
  return agentVersionAtLeast(version, [0, 59, 0])
}
