import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/windows',
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 30_000 },
  reporter: 'list',
});
