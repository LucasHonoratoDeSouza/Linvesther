import { readFile } from "node:fs/promises";
import path from "node:path";
import { requestSite } from "../../lib/requestOrigin";
import { docsOriginOf, llmsTxt } from "../../lib/siteMeta";

// The documents this is built from live in the repository, so the text here
// is always the text that is published there.
const SOURCES = [
  "README.md",
  "docs/architecture.md",
  "SECURITY.md",
  "CHANGELOG.md",
];

async function readSource(name: string): Promise<string | null> {
  try {
    return await readFile(path.join(process.cwd(), "..", "..", name), "utf8");
  } catch {
    return null;
  }
}

export async function GET() {
  const { origin } = await requestSite();
  const parts = [
    llmsTxt(docsOriginOf(origin, process.env.ROOT_DOMAIN), origin),
  ];
  for (const name of SOURCES) {
    const text = await readSource(name);
    if (text) parts.push(`\n---\n\n<!-- ${name} -->\n\n${text.trim()}\n`);
  }
  return new Response(parts.join("\n"), {
    headers: { "content-type": "text/plain; charset=utf-8" },
  });
}
