import { requestSite } from "../../../lib/requestOrigin";
import { securityTxt } from "../../../lib/siteMeta";

export async function GET() {
  const { origin } = await requestSite();
  return new Response(securityTxt(origin, new Date()), {
    headers: { "content-type": "text/plain; charset=utf-8" },
  });
}
