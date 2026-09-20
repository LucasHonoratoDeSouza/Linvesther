/** Whether a path is a file from `public/` (an image, an icon, a manifest, a
 * PDF), which every host serves. Pages never have an extension, so a path that
 * ends in one is a file. */
export function isStaticAsset(pathname: string): boolean {
  const last = pathname.split("/").pop() ?? "";
  return /\.[A-Za-z0-9]{1,12}$/.test(last);
}
