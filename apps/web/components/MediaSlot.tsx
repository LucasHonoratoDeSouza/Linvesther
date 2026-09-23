"use client";

import { useEffect, useRef, useState } from "react";

/** The landing page walkthrough. The video has no sound, so it autoplays and
 * loops. People who ask their system for reduced motion get a paused video
 * with the browser's own controls instead. */
export function MediaSlot() {
  const video = useRef<HTMLVideoElement>(null);
  const [reducedMotion, setReducedMotion] = useState(false);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setReducedMotion(true);
      video.current?.pause();
    }
  }, []);

  return (
    <div className="media-frame">
      <video
        ref={video}
        className="media-video"
        autoPlay
        muted
        loop
        playsInline
        preload="auto"
        controls={reducedMotion}
        poster="/video/linvesther-walkthrough-poster.webp"
        aria-label="Linvesther product walkthrough: from a track record to a statement of trust"
      >
        <source src="/video/linvesther-walkthrough.mp4" type="video/mp4" />
      </video>
    </div>
  );
}
