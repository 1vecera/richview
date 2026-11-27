// The pinned Playwright runtime bundles its PNG decoder without a declaration.
declare module 'playwright-core/lib/utilsBundle' {
  export const PNG: {
    sync: {
      read(input: Buffer): { width: number; height: number; data: Buffer };
    };
  };
}
