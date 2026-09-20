export function hostRewrites(rootDomain: string | undefined): never[] | { beforeFiles: { source: string; has: { type: "host"; value: string }[]; destination: string }[] };
