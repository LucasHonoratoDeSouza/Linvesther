import { requestSite } from "../../lib/requestOrigin";
import { docsOriginOf, llmsTxt } from "../../lib/siteMeta";

export async function GET() {
  const { origin } = await requestSite();
  return new Response(
    llmsTxt(docsOriginOf(origin, process.env.ROOT_DOMAIN), origin),
    { headers: { "content-type": "text/plain; charset=utf-8" } },
  );
}
