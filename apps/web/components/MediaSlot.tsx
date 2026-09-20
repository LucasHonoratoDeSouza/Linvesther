"use client";

import { useEffect, useRef, useState } from "react";

/** The landing page walkthrough. It autoplays muted, which browsers allow; the
 * button turns the sound on, which only a click can do. People who ask their
 * system for reduced motion get a paused video with the browser's own
 * controls instead. */
export function MediaSlot() {
  const video = useRef<HTMLVideoElement>(null);
  const [muted, setMuted] = useState(true);
  const [reducedMotion, setReducedMotion] = useState(false);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setReducedMotion(true);
      video.current?.pause();
    }
  }, []);

  function toggleSound() {
    const element = video.current;
    if (!element) return;
    element.muted = !element.muted;
    setMuted(element.muted);
  }

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
        poster="/video/linvesther-walkthrough-poster.jpg"
        aria-label="Linvesther product walkthrough: from a track record to a statement of trust"
      >
        <source src="/video/linvesther-walkthrough.mp4" type="video/mp4" />
      </video>
      {!reducedMotion && (
        <button
          type="button"
          className="media-sound"
          onClick={toggleSound}
          aria-pressed={!muted}
          aria-label={muted ? "Turn sound on" : "Turn sound off"}
          data-testid="video-sound-toggle"
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M11 5 6 9H3v6h3l5 4z" />
            {muted ? (
              <path d="m16 9 5 6m0-6-5 6" />
            ) : (
              <path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" />
            )}
          </svg>
        </button>
      )}
    </div>
  );
}
