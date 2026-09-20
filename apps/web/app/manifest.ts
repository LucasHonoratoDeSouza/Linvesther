import type { MetadataRoute } from "next";

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: "Linvesther",
    short_name: "Linvesther",
    description: "Turn your financial track record into verifiable claims, without publishing a balance, a position or a trade.",
    start_url: "/",
    display: "standalone",
    background_color: "#0b0d0c",
    theme_color: "#0b0d0c",
    icons: [
      { src: "/icons/icon-192.png", sizes: "192x192", type: "image/png" },
      { src: "/icons/icon-512.png", sizes: "512x512", type: "image/png" },
    ],
  };
}
