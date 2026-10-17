import { open } from "node:fs/promises";
import { resolve } from "node:path";
const files = {
  "/research/rankings": ["market_rankings.json", "application/json"],
  "/research/rankings.csv": ["market_rankings.csv", "text/csv; charset=utf-8"],
  "/research/summary": ["research_summary.md", "text/markdown; charset=utf-8"],
  "/research/fees": ["fee_sensitivity.md", "text/markdown; charset=utf-8"],
} as const;
export function isResearchRoute(path: string): path is keyof typeof files {
  return Object.hasOwn(files, path);
}
export async function researchResponse(
  directory: string,
  path: keyof typeof files,
) {
  const [name, contentType] = files[path];
  const file = await open(resolve(directory, name), "r");
  let body: string;
  try {
    const buffer = Buffer.alloc(8 * 1024 * 1024 + 1);
    const { bytesRead } = await file.read(buffer, 0, buffer.length, 0);
    if (bytesRead > 8 * 1024 * 1024)
      throw new Error("Research report too large");
    body = buffer.subarray(0, bytesRead).toString("utf8");
  } finally {
    await file.close();
  }
  if (path === "/research/rankings") {
    const report = JSON.parse(body);
    if (
      report.schema_version !== 1 ||
      report.preliminary !== true ||
      report.observations_are_real !== true ||
      !Number.isSafeInteger(report.generated_at_ms) ||
      !Array.isArray(report.rankings) ||
      report.rankings.length > 20 ||
      report.rankings.some(
        (r: { executable_edge_validated?: boolean }) =>
          r.executable_edge_validated !== false,
      )
    )
      throw new Error("Invalid research report");
  }
  return { body, contentType };
}
