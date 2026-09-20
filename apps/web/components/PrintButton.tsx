"use client";

import { Icon } from "./Icon";

export function PrintButton() {
  return (
    <button
      className="button button-outline print-button"
      onClick={() => window.print()}
    >
      Print / Save as PDF <Icon name="diagonal" width={15} />
    </button>
  );
}
