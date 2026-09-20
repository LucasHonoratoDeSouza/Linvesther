export function MediaSlot() {
  return (
    <video
      className="media-video"
      autoPlay
      muted
      loop
      playsInline
      preload="auto"
      poster="/video/linvesther-walkthrough-poster.jpg"
      aria-label="Linvesther product walkthrough: from a track record to a statement of trust"
    >
      <source src="/video/linvesther-walkthrough.mp4" type="video/mp4" />
    </video>
  );
}
